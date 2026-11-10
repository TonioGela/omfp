//! A native protocol client (`tokio-tungstenite`) for the daemon's integration (L2) and contract (L3) tests.

#![allow(dead_code)] // Each test binary uses a different subset.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use omfp_core::{ThreadEvent, ThreadState};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

/// How long a test waits for a frame before failing.
pub const WAIT: Duration = Duration::from_secs(60);

pub struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
    /// Frames read while waiting for a response.
    buffered: VecDeque<Value>,
}

pub fn url(addr: SocketAddr) -> String {
    format!("ws://{addr}/api/ws")
}

impl Client {
    /// Opens a connection without saying hello.
    pub async fn connect(addr: SocketAddr) -> Self {
        let (ws, _) = connect_async(url(addr)).await.unwrap();
        Self::from(ws)
    }

    /// Opens a connection with an `Origin` header.
    pub async fn connect_with_origin(
        addr: SocketAddr,
        origin: &str,
    ) -> Result<Self, tungstenite::Error> {
        let mut request = url(addr).into_client_request().unwrap();
        request
            .headers_mut()
            .insert("origin", origin.parse().unwrap());
        connect_async(request).await.map(|(ws, _)| Self::from(ws))
    }

    fn from(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self {
            ws,
            next_id: 0,
            buffered: VecDeque::new(),
        }
    }

    /// Connects and authenticates as `client`.
    pub async fn hello(addr: SocketAddr, token: &str, client: &str) -> Self {
        let mut connection = Self::connect(addr).await;
        let result = connection
            .call(
                "system.hello",
                json!({ "token": token, "protocol": 1, "client": client }),
            )
            .await;
        assert!(result.is_ok(), "hello failed: {result:?}");
        connection
    }

    pub async fn send_text(&mut self, text: String) {
        self.ws.send(Message::Text(text.into())).await.unwrap();
    }

    /// Sends a request and returns its response frame; other frames are kept for [`Client::frame`].
    pub async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = format!("c{}", self.next_id);
        self.send_text(
            json!({ "type": "request", "id": id, "method": method, "params": params }).to_string(),
        )
        .await;
        loop {
            let frame = self
                .read()
                .await
                .unwrap_or_else(|| panic!("connection closed waiting for {method}"));
            if frame["type"] == "response" && frame["id"] == id {
                return frame;
            }
            self.buffered.push_back(frame);
        }
    }

    /// The result, or the error object.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let response = self.request(method, params).await;
        if response["ok"] == true {
            Ok(response["result"].clone())
        } else {
            Err(response["error"].clone())
        }
    }

    /// The result; panics with the error.
    pub async fn ok(&mut self, method: &str, params: Value) -> Value {
        self.call(method, params)
            .await
            .unwrap_or_else(|error| panic!("{method} failed: {error}"))
    }

    /// The next frame that is not the response to one of our requests.
    pub async fn frame(&mut self) -> Value {
        if let Some(frame) = self.buffered.pop_front() {
            return frame;
        }
        self.read().await.expect("connection closed")
    }

    /// The next event of `topic`.
    pub async fn event(&mut self, topic: &str) -> Value {
        loop {
            let frame = self.frame().await;
            if frame["type"] == "event" && frame["topic"] == topic {
                return frame;
            }
        }
    }

    /// Reads one text frame; `None` once closed.
    async fn read(&mut self) -> Option<Value> {
        loop {
            let message = tokio::time::timeout(WAIT, self.ws.next())
                .await
                .expect("timed out waiting for a frame")?;
            match message.ok()? {
                Message::Text(text) => return Some(serde_json::from_str(&text).unwrap()),
                Message::Close(_) => return None,
                _ => {}
            }
        }
    }

    /// Reads until the daemon closes the connection; returns its close code.
    pub async fn close_code(&mut self) -> Option<u16> {
        loop {
            let message = tokio::time::timeout(WAIT, self.ws.next())
                .await
                .expect("timed out waiting for the close")?;
            match message {
                Ok(Message::Close(frame)) => return frame.map(|frame| frame.code.into()),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }

    /// Frames received but not consumed yet.
    pub fn take_buffered(&mut self) -> Vec<Value> {
        self.buffered.drain(..).collect()
    }
}

/// A thread followed from a subscription: the snapshot, then every event applied with the shared reducer (the
/// client side of PRT-021).
pub struct Follower {
    pub topic: String,
    pub epoch: String,
    pub seq: u64,
    pub state: ThreadState,
}

impl Follower {
    /// Subscribes to `thread/<id>` from `since`, expecting a snapshot.
    pub async fn snapshot(
        client: &mut Client,
        thread_id: &str,
        since: Option<(&str, u64)>,
    ) -> Self {
        let topic = format!("thread/{thread_id}");
        let mut params = json!({ "topic": topic });
        if let Some((epoch, seq)) = since {
            params["since"] = json!({ "epoch": epoch, "seq": seq });
        }
        let result = client.ok("subscribe", params).await;
        assert_eq!(result["mode"], "snapshot", "{result}");
        Self {
            topic,
            epoch: result["epoch"].as_str().unwrap().to_owned(),
            seq: result["seq"].as_u64().unwrap(),
            state: serde_json::from_value(result["state"].clone()).unwrap(),
        }
    }

    /// Applies one event frame; checks it continues the stream without a gap (PRT-022).
    pub fn apply(&mut self, frame: &Value) {
        let epoch = frame["epoch"].as_str().unwrap();
        let seq = frame["seq"].as_u64().unwrap();
        if epoch == self.epoch {
            assert_eq!(seq, self.seq + 1, "gap in {}: {frame}", self.topic);
        } else {
            // A new process: numbering restarts (PRT-022).
            assert_eq!(seq, 1, "a new epoch starts at 1: {frame}");
            epoch.clone_into(&mut self.epoch);
        }
        self.seq = seq;
        let event: ThreadEvent = serde_json::from_value(frame["event"].clone()).unwrap();
        self.state.apply(&event);
    }

    /// Applies events until `done` holds.
    pub async fn until(&mut self, client: &mut Client, done: impl Fn(&ThreadState) -> bool) {
        while !done(&self.state) {
            let frame = client.event(&self.topic).await;
            self.apply(&frame);
        }
    }
}
