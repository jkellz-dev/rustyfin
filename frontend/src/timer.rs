//! The browser's timer, for code that waits (toasts, polling).

/// Wait `ms` milliseconds.
pub async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// How long a toast stays on screen.
pub const TOAST_MS: i32 = 3000;
