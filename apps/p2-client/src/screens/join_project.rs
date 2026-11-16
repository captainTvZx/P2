//! Davet koduyla projeye katıl (docs/proje-bazli-yapi.md): kod gir → `GET /invites/:code` önizleme kartı
//! (ad, ikon, açıklama, üye sayısı, geçerlilik/sebep) → `POST /projects/join`. Proje rayındaki "＋"
//! menüsünden açılan pencere (`JoinDialog`) ve başlangıç ekranının katılma adımı aynı kartı kullanır
//! (`preview_card`). Görünüm ui/Modal.tsx (`.p2-modal`, cam kart, `PopElastic`) ile aynı kabuktur.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, FontWeight, MouseButton, Subscription, Window, div, px, relative};
use p2_client_core::{Api, ApiError, InvitePreview, JoinResult, community_error_text};
use p2_ui::P2Fx as _;
use p2_ui::P2Text as _;
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, kicker, lh, modal, modal_body, modal_foot};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT_MONO, Palette, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use crate::task;

/// Başlangıç ekranı `P2_AUTOPILOT_JOIN` kancasını kullandıysa çalışma alanı aynı kodu yeniden açmaz.
pub static AUTOPILOT_JOIN_USED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Hata kodunun arayüz metni (davet/katılma).
pub fn join_error(e: &ApiError) -> String {
    community_error_text(&e.code).map(str::to_string).unwrap_or_else(|| if e.message.is_empty() { "Projeye katılınamadı.".into() } else { e.message.clone() })
}

/// Önizlemedeki geçersizlik sebebi.
pub fn reason_text(reason: Option<&str>) -> &'static str {
    match reason {
        Some("INVITE_EXPIRED") => "Bu davetin süresi dolmuş.",
        Some("INVITE_FULL") => "Bu davetin kullanım sınırı dolmuş.",
        Some("INVITE_REVOKED") => "Bu davet kodu iptal edilmiş.",
        _ => "Bu davet artık geçerli değil.",
    }
}

/// Kod biçimi: `P2-` + 8 onaltılık (büyük/küçük harf duyarsız; boşluklar atılır).
pub fn normalize_code(raw: impl AsRef<str>) -> String {
    raw.as_ref().trim().replace(' ', "").to_uppercase()
}

fn code_complete(code: &str) -> bool {
    let c = normalize_code(code);
    c.len() == 11 && c.starts_with("P2-") && c[3..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Önizleme kartı: kapak şeridi (varsa), proje ikonu (ton gradyanı ya da görsel), ad, açıklama,
/// üye sayısı ve geçerlilik rozeti. `width` kartın dış genişliği.
pub fn preview_card(preview: &InvitePreview, width: f32, p: &Palette) -> AnyElement {
    let pr = &preview.project;
    let key = format!("invite-icon:{}:{}", pr.id, pr.icon_url.clone().unwrap_or_default());
    crate::avatars::remember(&key, pr.icon_url.as_deref());
    let face = crate::avatars::image(&key)
        .filter(|_| pr.icon_url.is_some())
        .map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(18.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element())
        .unwrap_or_else(|| p2_ui::components::initials_label(p2_ui::components::user_initials(&pr.name)).into_any_element());
    let hue = p.hues[p2_ui::components::avatar_hue(&pr.id) % 10];
    let cover_key = format!("invite-cover:{}:{}", pr.id, pr.cover_url.clone().unwrap_or_default());
    crate::avatars::remember(&cover_key, pr.cover_url.as_deref());
    let cover = pr.cover_url.as_ref().and_then(|_| crate::avatars::image(&cover_key)).map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().object_fit(gpui_kit::ObjectFit::Cover).into_any_element());
    let banner_h = 64.;
    let banner = div()
        .relative()
        .w_full()
        .h(px(banner_h))
        .overflow_hidden()
        .bg(gpui_kit::linear_gradient(135., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(p.accent_soft_2, 1.)))
        .children(cover);
    let (badge_bg, badge_fg, badge_border, badge_text) = if preview.valid { (p.success_soft, p.success_text, p.success_border, "Geçerli davet".to_string()) } else { (p.danger_soft, p.danger_text, p.danger_border, reason_text(preview.reason.as_deref()).to_string()) };
    let uses = match (preview.max_uses, preview.expires_at.as_deref()) {
        (Some(max), _) => Some(format!("{}/{max} kullanım", preview.use_count)),
        (None, Some(_)) => Some("Süreli davet".to_string()),
        _ => None,
    };
    div()
        .w(px(width))
        .overflow_hidden()
        .border_1()
        .border_color(if preview.valid { p.border_strong } else { p.danger_border })
        .rounded(radius::LG)
        .bg(p.bg_2)
        .shadow(shadows(&[p.inset_top]))
        .child(banner)
        .child(
            div()
                .relative()
                .px(px(16.))
                .pb(px(14.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .mt(px(-30.))
                        .size(px(60.))
                        .flex_none()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_4()
                        .border_color(p.bg_2)
                        .rounded(px(20.))
                        .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                        .text_color(hue.fg)
                        .text_size(fs(18.))
                        .line_height(px(52.))
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .child(face),
                )
                .child(kicker("PROJE DAVETİ", p).line_height(lh(font_size::XXS)))
                .child(div().text_size(fs(17.)).line_height(lh(fs(17.))).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(pr.name.clone()))
                .when_some(pr.description.clone().filter(|d| !d.trim().is_empty()), |s, d| s.child(div().text_size(fs(11.5)).line_height(relative(1.5)).text_color(p.text_2).child(d)))
                .child(
                    div()
                        .mt(px(4.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(div().size(px(8.)).rounded_full().bg(p.success))
                                .child(div().text_size(fs(11.)).css_weight(650.).text_color(p.text_2).child(format!("{} üye", pr.member_count))),
                        )
                        .when_some(uses, |s, u| s.child(div().text_size(fs(11.)).text_color(p.text_3).child(format!("· {u}"))))
                        .child(div().ml_auto().px(px(9.)).py(px(3.)).border_1().border_color(badge_border).rounded(radius::PILL).bg(badge_bg).text_color(badge_fg).text_size(fs(9.5)).line_height(lh(fs(9.5))).font_weight(FontWeight::BOLD).child(badge_text)),
                ),
        )
        .p2_enter(gpui_kit::SharedString::from(format!("invite-card-{}", preview.code)), Entrance::Rise, Duration::ZERO)
        .into_any_element()
}

pub enum JoinEvent {
    Close,
    Joined(JoinResult),
}

pub struct JoinDialog {
    api: Arc<Api>,
    code: Entity<InputState>,
    preview: Option<InvitePreview>,
    /// Önizlenen kod (yazılan kod değişince kart düşer).
    previewed: String,
    error: Option<String>,
    busy: bool,
    /// Otomatik pilot: önizleme bitince katıl.
    auto_join: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<JoinEvent> for JoinDialog {}

impl JoinDialog {
    pub fn new(api: Arc<Api>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("P2-XXXXXXXX"));
        code.update(cx, |s, cx| s.focus(window, cx));
        let subs = vec![cx.subscribe_in(&code, window, |this, state, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    let v = state.read(cx).value().to_string();
                    if v.chars().count() > 20 {
                        let cut: String = v.chars().take(20).collect();
                        state.update(cx, |s, cx| s.set_value(cut, window, cx));
                        return;
                    }
                    if normalize_code(&v) != this.previewed {
                        this.preview = None;
                        this.error = None;
                    }
                    // Tam kod yazıldı/yapıştırıldı: kendiliğinden önizle.
                    if code_complete(&v) && normalize_code(&v) != this.previewed {
                        this.load_preview(window, cx);
                    }
                }
                InputEvent::PressEnter { .. } => {
                    if this.preview.as_ref().is_some_and(|p| p.valid) {
                        this.join(window, cx);
                    } else {
                        this.load_preview(window, cx);
                    }
                }
                _ => {}
            }
            cx.notify();
        })];
        Self { api, code, preview: None, previewed: String::new(), error: None, busy: false, auto_join: false, _subs: subs }
    }

    /// Otomatik pilot: kodu yaz, önizle (`join` ise ardından katıl).
    pub fn autopilot(&mut self, code: &str, join: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.auto_join = join;
        let code = code.to_string();
        self.code.update(cx, |s, cx| s.set_value(code, window, cx));
        self.load_preview(window, cx);
    }

    fn load_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = normalize_code(self.code.read(cx).value());
        if code.len() < 6 || self.busy {
            if code.len() < 6 {
                self.error = Some("Davet kodunu gir (örn. P2-1A2B3C4D).".into());
                cx.notify();
            }
            return;
        }
        self.busy = true;
        self.error = None;
        self.previewed = code.clone();
        cx.notify();
        let api = self.api.clone();
        task::run(window, cx, move || api.invite_preview(&code), |this, r, window, cx| {
            this.busy = false;
            match r {
                Ok(preview) => {
                    crate::trace(&format!("katıl: önizleme {} ({} üye, {})", preview.project.name, preview.project.member_count, if preview.valid { "geçerli".to_string() } else { preview.reason.clone().unwrap_or_default() }));
                    let go = preview.valid && this.auto_join;
                    this.preview = Some(preview);
                    if go {
                        this.join(window, cx);
                    }
                }
                Err(e) => {
                    crate::trace(&format!("katıl: hata {}", e.code));
                    this.preview = None;
                    this.error = Some(join_error(&e));
                }
            }
            cx.notify();
        });
    }

    fn join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preview) = self.preview.clone().filter(|p| p.valid) else { return };
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let api = self.api.clone();
        let code = preview.code.clone();
        task::run(window, cx, move || api.join_project(&code), |this, r, _, cx| {
            this.busy = false;
            match r {
                Ok(joined) => {
                    crate::trace(&format!("katıl: {} projesine {}", joined.project.name, if joined.joined { "katıldın" } else { "zaten üyesin" }));
                    cx.emit(JoinEvent::Joined(joined));
                }
                Err(e) => {
                    crate::trace(&format!("katıl: hata {}", e.code));
                    this.error = Some(join_error(&e));
                }
            }
            cx.notify();
        });
    }
}

impl Render for JoinDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let focused = gpui_kit::Focusable::focus_handle(self.code.read(cx), cx).is_focused(window);
        let close = button("join-x", BtnVariant::Ghost, BtnSize::Sm, true, false, &p).text_size(font_size::MD).child("×").on_click(cx.listener(|_, _, _, cx| cx.emit(JoinEvent::Close)));
        let head = div()
            .flex()
            .items_start()
            .gap(px(12.))
            .pt(px(18.))
            .pr(px(16.))
            .pb(px(12.))
            .pl(px(20.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(kicker("DAVETLE KATIL", &p).line_height(lh(font_size::XXS)))
                    .child(div().text_size(font_size::XL).font_weight(FontWeight::BOLD).text_color(p.text_strong).line_height(relative(1.25)).child("Bir projeye katıl"))
                    .child(div().mt(px(3.)).text_size(font_size::SM).text_color(p.text_3).line_height(relative(1.4)).child("Proje yöneticisinin paylaştığı kodu gir. Kod yalnız o projeye sokar.")),
            )
            .child(close);
        let field = div()
            .h(px(44.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(radius::MD)
            .border_1()
            .border_color(if self.error.is_some() { p.danger_border } else if focused { p.border_accent } else { p.border })
            .bg(p.bg_3)
            .when(focused, |s| s.shadow(shadows(&[p.ring])))
            .child(div().flex_none().text_color(p.text_accent).font_family(FONT_MONO).text_size(fs(13.)).font_weight(FontWeight::BOLD).child("#"))
            .child(Input::new(&self.code).appearance(false).bordered(false).focus_bordered(false).p(px(0.)).font_family(FONT_MONO).text_size(fs(14.)).flex_1());
        let card_w = 590f32.min(f32::from(window.viewport_size().width) - 48.) - 42.;
        let body = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().flex().flex_col().gap(px(6.)).child(div().text_size(font_size::SM).line_height(lh(font_size::SM)).css_weight(600.).text_color(p.text_3).child("Davet kodu")).child(field))
            .when_some(self.error.clone(), |s, e| {
                s.child(
                    div()
                        .flex()
                        .items_start()
                        .gap(px(10.))
                        .px(px(12.))
                        .py(px(10.))
                        .border_1()
                        .border_color(p.danger_border)
                        .rounded(radius::MD)
                        .bg(p.danger_soft)
                        .child(div().flex_none().size(px(20.)).flex().items_center().justify_center().rounded_full().bg(p.danger).text_color(gpui_kit::white()).text_size(fs(11.)).font_weight(FontWeight::EXTRA_BOLD).child("!"))
                        .child(div().flex_1().text_size(fs(11.5)).line_height(relative(1.5)).text_color(p.danger_text).child(e))
                        .p2_enter("join-error", Entrance::Rise, Duration::ZERO),
                )
            })
            .when_some(self.preview.as_ref(), |s, pv| s.child(preview_card(pv, card_w, &p)))
            .when(self.preview.is_none() && self.error.is_none(), |s| {
                s.child(div().text_size(fs(10.5)).line_height(relative(1.5)).text_color(p.text_3).child(if self.busy { "Davet aranıyor…" } else { "Kodu yapıştırınca proje kartı burada görünür; katılmadan önce hangi projeye girdiğini görürsün." }))
            });
        let can_join = self.preview.as_ref().is_some_and(|p| p.valid) && !self.busy;
        let cancel = button("join-cancel", BtnVariant::Secondary, BtnSize::Md, false, false, &p).child("Vazgeç").on_click(cx.listener(|_, _, _, cx| cx.emit(JoinEvent::Close)));
        let primary = if self.preview.is_some() {
            button("join-go", BtnVariant::Primary, BtnSize::Md, false, false, &p).when(!can_join, |s| s.opacity(0.5)).child(if self.busy { "Katılınıyor…" } else { "Projeye katıl" }).on_click(cx.listener(move |this, _, window, cx| {
                if can_join {
                    this.join(window, cx)
                }
            }))
        } else {
            button("join-preview", BtnVariant::Primary, BtnSize::Md, false, false, &p).when(self.busy, |s| s.opacity(0.5)).child("Kodu kontrol et").on_click(cx.listener(|this, _, window, cx| this.load_preview(window, cx)))
        };
        let card = modal(ModalSize::Md, window, &p)
            .id("join-card")
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(head)
            .child(modal_body().child(body))
            .child(modal_foot(&p).child(cancel).child(primary));
        div()
            .id("join-layer")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .px(px(24.))
            .pb(px(24.))
            .bg(p.overlay)
            .backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(JoinEvent::Close)))
            .child(card.p2_enter("join", Entrance::PopElastic, Duration::ZERO))
    }
}
