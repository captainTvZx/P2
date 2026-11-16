//! Açılış ekranları (App.tsx bileşenlerinin karşılıkları).

mod auth;
mod board;
mod board_editor;
mod admin;
mod channel_perms;
mod chat;
mod thread;
mod documents;
mod group_creator;
mod library;
/// Kütüphane biçimlendiricileri (H): `settings_library.rs` `super::super::library_ui::fmt_bytes` kullanır.
pub(crate) use library::ui as library_ui;
mod mode;
mod join_project;
mod onboarding;
mod problem;
pub(crate) mod settings;
mod setup;
pub(crate) mod setup_ui;
mod workspace;

pub use auth::AuthScreen;
pub use mode::ModeScreen;
pub use onboarding::OnboardingScreen;
pub use problem::ProblemScreen;
pub use setup::SetupAdminScreen;
pub use workspace::WorkspaceScreen;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, Subscription, Window};

/// Tek satırlık metin alanı.
pub fn text_input<V: 'static>(window: &mut Window, cx: &mut Context<V>, placeholder: &str) -> Entity<InputState> {
    let placeholder = placeholder.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

/// Parola alanı (maskeli).
pub fn password_input<V: 'static>(window: &mut Window, cx: &mut Context<V>) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).masked(true))
}

/// Alanda Enter'a basılınca `submit` çağrılır (HTML form gönderimi gibi).
pub fn submit_on_enter<V: 'static>(
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<V>,
    submit: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Subscription {
    cx.subscribe_in(input, window, move |this, _, event: &InputEvent, window, cx| {
        if let InputEvent::PressEnter { .. } = event {
            submit(this, window, cx);
        }
    })
}

/// P2 görünümlü girdi: alanın kendisi gpui-component'ten, kenarlık temadan.
pub fn styled_input(state: &Entity<InputState>, password: bool) -> AnyElement {
    let input = Input::new(state);
    if password { input.mask_toggle().into_any_element() } else { input.into_any_element() }
}

pub fn value(state: &Entity<InputState>, cx: &gpui_kit::App) -> String {
    state.read(cx).value().to_string()
}
