//! The browser host: the access token (UI-002), the client over the page's own origin, per-frame rendering with
//! `requestAnimationFrame` (UI-012), and intents dispatched to the daemon.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use omfp_client::{Client, ClientConfig, GlooTimer, WebTransport, connect};
use omfp_protocol::Topic;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};

use super::dispatch::Dispatcher;
use super::frame::{ClientSource, Frames, render_loop};
use super::token;
use super::view::ViewSync;
use crate::input::{ConnectionView, ViewInput};
use crate::intent::Intent;
use crate::views::app::App;
use crate::views::token::TokenPrompt;

/// `localStorage` key of the access token.
const TOKEN_KEY: &str = "omfp.token";

/// Reads `#token=…` and removes it from the address bar with `history.replaceState` (UI-002).
fn take_fragment_token() -> Option<String> {
    let window = web_sys::window()?;
    let location = window.location();
    let hash = location.hash().ok()?;
    let token = token::fragment_token(&hash)?;
    let url = format!(
        "{}{}{}",
        location.pathname().unwrap_or_default(),
        location.search().unwrap_or_default(),
        token::fragment_without_token(&hash)
    );
    if let Ok(history) = window.history() {
        // Best effort: the token is used either way.
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&url));
    }
    Some(token)
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn stored_token() -> Option<String> {
    storage()?.get_item(TOKEN_KEY).ok()?
}

fn store_token(token: &str) {
    if let Some(storage) = storage() {
        // Without storage (private mode) the token lasts for this page only.
        let _ = storage.set_item(TOKEN_KEY, token);
    }
}

/// The token to start with: the address bar's (then stored), else the stored one.
fn initial_token() -> Option<String> {
    let fresh = take_fragment_token();
    if let Some(token) = &fresh {
        store_token(token);
    }
    token::choose(fresh, stored_token())
}

/// The daemon's WebSocket on the origin that served the page.
fn socket_url() -> String {
    let (protocol, host) = web_sys::window()
        .map(|window| {
            let location = window.location();
            (
                location.protocol().unwrap_or_default(),
                location.host().unwrap_or_default(),
            )
        })
        .unwrap_or_default();
    let scheme = if protocol == "https:" { "wss" } else { "ws" };
    format!("{scheme}://{host}/api/ws")
}

/// `requestAnimationFrame`.
struct AnimationFrames;

impl Frames for AnimationFrames {
    fn next(&self) -> impl Future<Output = ()> {
        let (fired, frame) = futures::channel::oneshot::channel();
        // Freed by the browser after it ran once.
        let callback = Closure::once_into_js(move |_: f64| {
            let _ = fired.send(());
        });
        let requested = web_sys::window().is_some_and(|window| {
            window
                .request_animation_frame(callback.unchecked_ref())
                .is_ok()
        });
        async move {
            if requested {
                let _ = frame.await;
            }
        }
    }
}

/// One connection to the daemon and the window's dispatcher.
struct Host {
    client: Client,
    dispatcher: Dispatcher<Client>,
    sync: Rc<RefCell<ViewSync>>,
}

impl Host {
    /// Starts `intent`; its effects land in `input` when the daemon answered, even if the component that sent it is
    /// gone by then.
    fn dispatch(&mut self, intent: Intent, mut input: Signal<ViewInput>) {
        let effects = self.dispatcher.dispatch(intent);
        let sync = Rc::clone(&self.sync);
        dioxus::core::spawn_forever(async move {
            for effect in effects.await {
                sync.borrow_mut().apply_effect(&mut input.write(), effect);
            }
        });
    }
}

/// The current connection, shared by the root (which dispatches) and the connection component (which replaces it).
#[derive(Clone, Default)]
struct SharedHost(Rc<RefCell<Option<Host>>>);

impl PartialEq for SharedHost {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The web app: the token prompt when needed, and the app over the daemon's data.
#[component]
pub fn Root() -> Element {
    let mut token = use_signal(initial_token);
    // Bumped on every submitted token, so the same token can be tried again.
    let mut attempt = use_signal(|| 0_u64);
    let mut input = use_signal(ViewInput::default);
    let host: SharedHost = use_hook(SharedHost::default);

    let rejected = input.read().connection == ConnectionView::Unauthenticated;
    let prompt = rejected || token.read().is_none();
    let dispatch_host = host.clone();
    rsx! {
        if let Some(token) = token() {
            Connection { key: "{attempt}", token, input, host: host.clone() }
        }
        div { style: "display: contents", inert: if prompt { "true" },
            App {
                input,
                on_intent: move |intent| {
                    if let Some(host) = dispatch_host.0.borrow_mut().as_mut() {
                        host.dispatch(intent, input);
                    }
                },
            }
        }
        if prompt {
            TokenPrompt {
                rejected,
                on_submit: move |value: String| {
                    store_token(&value);
                    {
                        let mut input = input.write();
                        input.connection = ConnectionView::Connecting;
                        input.open.clear();
                    }
                    attempt += 1;
                    token.set(Some(value));
                },
            }
        }
    }
}

/// Connects with `token` while mounted: the client's driver and the per-frame render loop run in this scope and stop
/// with it.
#[component]
fn Connection(token: String, input: Signal<ViewInput>, host: SharedHost) -> Element {
    let client = use_hook(|| {
        let url = socket_url();
        let (client, driver) = connect(
            move || WebTransport::connect(url.clone()),
            GlooTimer,
            ClientConfig::new(token.clone(), "omfp-ui/web"),
        );
        client.subscribe(Topic::Index);
        spawn(driver);
        let sync = Rc::new(RefCell::new(ViewSync::default()));
        let rendered = Rc::clone(&sync);
        let mut target = input;
        spawn(render_loop(
            ClientSource::new(client.clone()),
            AnimationFrames,
            move |model, dirty| {
                rendered
                    .borrow_mut()
                    .apply_model(&mut target.write(), model, dirty);
            },
        ));
        // A new connection keeps following the thread the window shows.
        let watched = host
            .0
            .borrow()
            .as_ref()
            .and_then(|previous| previous.dispatcher.watched().map(str::to_owned));
        let mut next = Host {
            client: client.clone(),
            dispatcher: Dispatcher::new(client.clone()),
            sync,
        };
        if let Some(thread_id) = watched {
            next.dispatch(Intent::SelectThread { thread_id }, input);
        }
        if let Some(previous) = host.0.borrow_mut().replace(next) {
            previous.client.close();
        }
        client
    });
    use_drop(move || client.close());
    rsx! {}
}
