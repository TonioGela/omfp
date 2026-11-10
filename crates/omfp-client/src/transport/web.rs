//! [`Transport`] over the browser's `WebSocket` (`web-sys`).

use std::cell::RefCell;
use std::rc::Rc;

use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, CloseEvent, Event, MessageEvent, WebSocket};

use super::{Closed, Incoming, Transport, TransportError};

type Opened = Rc<RefCell<Option<oneshot::Sender<Result<(), TransportError>>>>>;

/// A browser WebSocket. The browser sends the page's `Origin`, which the daemon checks (PRT-003).
#[derive(Debug)]
pub struct WebTransport {
    socket: WebSocket,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    closed: Option<Closed>,
    // Kept alive as long as the socket may call them.
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
    _on_error: Closure<dyn FnMut(Event)>,
}

fn js_error(error: &wasm_bindgen::JsValue) -> TransportError {
    TransportError(
        error
            .as_string()
            .or_else(|| {
                error
                    .dyn_ref::<js_sys::Error>()
                    .map(|error| String::from(error.message()))
            })
            .unwrap_or_else(|| format!("{error:?}")),
    )
}

fn settle(opened: &Opened, result: Result<(), TransportError>) {
    if let Some(sender) = opened.borrow_mut().take() {
        let _ = sender.send(result);
    }
}

impl WebTransport {
    /// Opens a WebSocket to `url` (`ws://host:port/api/ws`) and waits until it is open.
    pub async fn connect(url: String) -> Result<Self, TransportError> {
        let socket = WebSocket::new(&url).map_err(|error| js_error(&error))?;
        socket.set_binary_type(BinaryType::Arraybuffer);
        let (sender, incoming) = mpsc::unbounded();
        let (opened_tx, opened_rx) = oneshot::channel();
        let opened: Opened = Rc::new(RefCell::new(Some(opened_tx)));

        let on_open = {
            let opened = Rc::clone(&opened);
            Closure::<dyn FnMut(Event)>::new(move |_: Event| settle(&opened, Ok(())))
        };
        let on_message = {
            let sender = sender.clone();
            Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                // Binary frames carry terminal data (M6).
                if let Some(text) = event.data().as_string() {
                    let _ = sender.unbounded_send(Incoming::Text(text));
                }
            })
        };
        let on_close = {
            let opened = Rc::clone(&opened);
            Closure::<dyn FnMut(CloseEvent)>::new(move |event: CloseEvent| {
                let closed = Closed {
                    code: Some(event.code()),
                    reason: event.reason(),
                };
                settle(
                    &opened,
                    Err(TransportError(format!("cannot connect: {closed}"))),
                );
                let _ = sender.unbounded_send(Incoming::Closed(closed));
            })
        };
        // An error is always followed by a close event, which carries the details.
        let on_error = Closure::<dyn FnMut(Event)>::new(|_: Event| {});

        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        let transport = Self {
            socket,
            incoming,
            closed: None,
            _on_open: on_open,
            _on_message: on_message,
            _on_close: on_close,
            _on_error: on_error,
        };
        match opened_rx.await {
            Ok(Ok(())) => Ok(transport),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(TransportError("the socket went away while opening".into())),
        }
    }
}

impl Transport for WebTransport {
    fn send(&mut self, text: String) -> impl Future<Output = Result<(), TransportError>> {
        std::future::ready(
            self.socket
                .send_with_str(&text)
                .map_err(|error| js_error(&error)),
        )
    }

    async fn recv(&mut self) -> Incoming {
        if let Some(closed) = &self.closed {
            return Incoming::Closed(closed.clone());
        }
        let incoming = self.incoming.next().await.unwrap_or_else(|| {
            Incoming::Closed(Closed {
                code: None,
                reason: String::new(),
            })
        });
        if let Incoming::Closed(closed) = &incoming {
            self.closed = Some(closed.clone());
        }
        incoming
    }

    fn close(&mut self) -> impl Future<Output = ()> {
        let _ = self.socket.close();
        std::future::ready(())
    }
}

impl Drop for WebTransport {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        self.socket.set_onerror(None);
        let _ = self.socket.close();
    }
}
