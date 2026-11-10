//! Platform services: wall clock and timers for the countdown (UI-073), and image files from a paste event (UI-060).
//! The browser's on wasm, tokio's natively (desktop). Futures are only polled by a running renderer, never during
//! server-side rendering.

/// Milliseconds since the Unix epoch.
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> i64 {
    // `Date.now()` is an integral number of milliseconds.
    #[allow(clippy::cast_possible_truncation)]
    let now = js_sys::Date::now() as i64;
    now
}

/// Milliseconds since the Unix epoch.
#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// Waits `ms` milliseconds.
#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u32) {
    gloo_timers::future::TimeoutFuture::new(ms).await;
}

/// Waits `ms` milliseconds.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
}

/// Image files carried by a paste event. Only the browser exposes them (`ClipboardEvent.clipboardData.files`); the
/// desktop webview's paste events carry no files in Dioxus 0.7, so images reach the desktop composer by drag and drop.
#[cfg(target_arch = "wasm32")]
pub fn pasted_images(data: &dioxus::html::ClipboardData) -> Vec<web_sys::File> {
    use wasm_bindgen::JsCast;

    let files = data
        .downcast::<web_sys::Event>()
        .and_then(|event| event.dyn_ref::<web_sys::ClipboardEvent>())
        .and_then(web_sys::ClipboardEvent::clipboard_data)
        .and_then(|transfer| transfer.files());
    let Some(files) = files else {
        return Vec::new();
    };
    (0..files.length())
        .filter_map(|index| files.get(index))
        .filter(|file| file.type_().starts_with("image/"))
        .collect()
}

/// The MIME type and bytes of a browser file.
#[cfg(target_arch = "wasm32")]
pub async fn read_file(file: web_sys::File) -> Option<(String, Vec<u8>)> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .ok()?;
    Some((file.type_(), js_sys::Uint8Array::new(&buffer).to_vec()))
}
