//! Engelleyici ağ çağrılarını arka plan yürütücüsünde çalıştırıp sonucu pencere
//! bağlamında görünüme geri getirir.

use gpui_kit::{Context, Window};

/// `job` arka planda çalışır; `done` sonuçla birlikte görünümün kendi bağlamında çağrılır.
pub fn run<V: 'static, T: Send + 'static>(
    window: &Window,
    cx: &mut Context<V>,
    job: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(&mut V, T, &mut Window, &mut Context<V>) + 'static,
) {
    cx.spawn_in(window, async move |this, cx| {
        let result = cx.background_executor().spawn(async move { job() }).await;
        let _ = this.update_in(cx, move |view, window, cx| done(view, result, window, cx));
    })
    .detach();
}
