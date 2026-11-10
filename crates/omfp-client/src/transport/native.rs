//! [`Transport`] over tokio-tungstenite, for native hosts (desktop, scripts, tests).

use futures::{SinkExt, StreamExt};
use omfp_protocol::MAX_FRAME_BYTES;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async_with_config};

use super::{Closed, Incoming, Transport, TransportError};

/// A WebSocket connection opened with tokio-tungstenite. Needs a tokio runtime.
#[derive(Debug)]
pub struct NativeTransport {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    closed: Option<Closed>,
}

impl NativeTransport {
    /// Connects to `url` (`ws://host:port/api/ws`). Sends no `Origin` header: native clients are not browsers
    /// (PRT-003).
    pub async fn connect(url: String) -> Result<Self, TransportError> {
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_FRAME_BYTES))
            .max_frame_size(Some(MAX_FRAME_BYTES));
        let (socket, _) = connect_async_with_config(url.as_str(), Some(config), true)
            .await
            .map_err(|error| TransportError(format!("cannot connect to {url}: {error}")))?;
        Ok(Self {
            socket,
            closed: None,
        })
    }

    fn closed(&mut self, closed: Closed) -> Incoming {
        self.closed = Some(closed.clone());
        Incoming::Closed(closed)
    }
}

impl Transport for NativeTransport {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        self.socket
            .send(Message::Text(text.into()))
            .await
            .map_err(|error| TransportError(error.to_string()))
    }

    async fn recv(&mut self) -> Incoming {
        if let Some(closed) = &self.closed {
            return Incoming::Closed(closed.clone());
        }
        loop {
            let closed = match self.socket.next().await {
                Some(Ok(Message::Text(text))) => return Incoming::Text(text.as_str().to_owned()),
                // Pings are answered by tungstenite; binary frames carry terminal data (M6).
                Some(Ok(
                    Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_),
                )) => {
                    continue;
                }
                Some(Ok(Message::Close(frame))) => Closed {
                    code: frame.as_ref().map(|frame| u16::from(frame.code)),
                    reason: frame
                        .map(|frame| frame.reason.as_str().to_owned())
                        .unwrap_or_default(),
                },
                Some(Err(error)) => Closed {
                    code: None,
                    reason: error.to_string(),
                },
                None => Closed {
                    code: None,
                    reason: String::new(),
                },
            };
            return self.closed(closed);
        }
    }

    async fn close(&mut self) {
        let _ = self.socket.close(None).await;
    }
}
