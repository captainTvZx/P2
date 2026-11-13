//! Kişiler arayüzü (D ajanı) — ray avatarının profil kartı (Workspace.tsx `.rail-profile-card`,
//! project-rail.css 90–140) ve profil paneli (ProfilePanel.tsx, direct.css 45–86).
//! Boyutlar CSS px; yazılar `fs()` ölçekli (P2 Mavi 0,85).

use std::sync::atomic::Ordering;
use p2_ui::P2Fx as _; // backdrop_blur
use gpui_kit::prelude::*;
use gpui_kit::{Animation, AnimationExt as _, AnyElement, BoxShadow, Context, Hsla, MouseButton, SharedString, div, linear_color_stop, linear_gradient, point, px};
use p2_client_core::Member;
use p2_ui::{P2Motion as _, P2Text as _};
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::motion::{EASE_ELASTIC, EASE_POP, M_BASE, M_SLOW};
use p2_ui::tokens::{Palette, css_rgba, fs, shadows};

use super::ChatSubject;
use super::Section;
use super::workspace_nav::presence_mark;
use super::{PRESENCE_OPTIONS, WorkspaceScreen, role_label, status_label};

/// Chromium tek renkli simgeleri (🛡 ⚙ ✉) Segoe UI Symbol ile çizer (renkli emoji değil).
const SYMBOL_FONT: &str = "Segoe UI Symbol";

fn shadow(x: f32, y: f32, blur: f32, spread: f32, color: Hsla, inset: bool) -> BoxShadow {
    BoxShadow { color, offset: point(px(x), px(y)), blur_radius: px(blur), spread_radius: px(spread), inset }
}

/// direct.css `.presence` + `.online/.away/.busy`, `border: 0` biçimi (durum satırları; `position: static`).
pub(super) fn presence_flat(status: &str, size: f32, p: &Palette) -> gpui_kit::Div {
    let dot = div().flex_none().size(px(size)).rounded_full();
    match status {
        "ONLINE" => dot.bg(p.success).shadow(vec![shadow(0., 0., 0., 1., css_rgba(61, 220, 132, 0.35), false), shadow(0., 0., 7., 0., css_rgba(61, 220, 132, 0.45), false)]),
        // `background: bg-2; box-shadow: inset 0 0 0 2.5px warning`.
        "AWAY" => dot.bg(p.warning).p(px(2.5)).child(div().size_full().rounded_full().bg(p.bg_2)),
        "BUSY" => dot.bg(p.danger).shadow(vec![shadow(0., 0., 0., 1., css_rgba(255, 92, 108, 0.35), false)]),
        _ => dot.bg(p.text_4),
    }
}

/// direct.css `.profile-line` rengi (durumla).
fn line_color(status: &str, p: &Palette) -> Hsla {
    match status {
        "ONLINE" => p.success_text,
        "AWAY" => p.warning_text,
        "BUSY" => p.danger_text,
        _ => p.text_2,
    }
}

impl WorkspaceScreen {
    /// Ray avatarının profil kartı (Workspace.tsx `rail-profile-layer` + `.rail-profile-card`):
    /// tam ekran saydam katman (dışarı basınca kapanır), kart solda 78px, altta 12px.
    pub(super) fn user_menu_layer(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.user_menu {
            return None;
        }
        let user = &self.auth.user;
        let hue = p.hues[avatar_hue(&user.id)];
        let shown = if self.preference == "INVISIBLE" { "INVISIBLE".to_string() } else { self.my_status.clone() };
        let shown_label = if self.preference == "INVISIBLE" { "Görünmez" } else { status_label(&self.my_status) };
        let (soft, strong, selected) = (p.accent_soft, p.text_strong, p.bg_selected);

        let mut list = div().flex().flex_col().gap(px(2.)).mb(px(8.));
        for (status, label) in PRESENCE_OPTIONS {
            let on = self.preference == *status;
            list = list.child(
                div()
                    .id(SharedString::from(format!("presence-{status}")))
                    .w_full()
                    .h(px(30.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .text_size(fs(11.))
                    .line_height(p2_ui::text::normal_line_height(fs(11.)))
                    .css_weight(600.)
                    .text_color(if on { strong } else { p.text_2 })
                    .cursor_pointer()
                    .when(on, |s| s.bg(selected))
                    .hover(move |s| s.bg(soft).text_color(strong))
                    .child(*label)
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_presence(status, cx))),
            );
        }
        // Electron'da durum listesindeki `<i class="presence …">` noktaları `position: absolute`
        // olduğundan kartın sağ-alt köşesine (−3px) üst üste düşer; sonuncusu (Görünmez) üstte görünür.
        let mut stack = div().absolute().right(px(-3.)).bottom(px(-3.)).size(px(11.));
        for (status, _) in PRESENCE_OPTIONS {
            stack = stack.child(presence_mark(status, p.bg_2, p).absolute().top_0().left_0());
        }

        let staff = self.my_perms.can("admin.panel");
        let owner = self.org.role == "OWNER";
        let (bg_hover, border_strong, accent, on_accent) = (p.bg_hover, p.border_strong, p.accent, p.text_on_accent);
        let wide = |id: &'static str| {
            div()
                .id(id)
                .group(id)
                .w_full()
                .h(px(33.))
                .flex()
                .items_center()
                .justify_between()
                .mb(px(6.))
                .px(px(11.))
                .border_1()
                .rounded(px(8.))
                .text_size(fs(11.))
                .line_height(p2_ui::text::normal_line_height(fs(11.)))
                .css_weight(650.)
                .cursor_pointer()
        };
        let admin = staff.then(|| {
            wide("menu-admin")
                .border_color(p.border_accent)
                .bg(p.accent_soft)
                .text_color(p.text_strong)
                .hover(move |s| s.bg(accent).border_color(accent).text_color(on_accent))
                .child(div().flex().items_center().child(div().font_family(SYMBOL_FONT).child("🛡")).child(" Yönetim paneli"))
                .child(
                    div()
                        .text_size(fs(9.))
                        .css_weight(700.)
                        .tracking(0.04)
                        .text_color(p.text_accent)
                        .group_hover("menu-admin", move |s| s.text_color(on_accent))
                        .child(if owner { "SAHİP" } else { "YÖNETİCİ" }),
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_admin(window, cx)))
        });
        let settings = wide("menu-settings")
            .border_color(p.border)
            .bg(p.bg_3)
            .text_color(p.text)
            .hover(move |s| s.bg(bg_hover).border_color(border_strong).text_color(strong))
            .child(div().flex().items_center().child(div().font_family(SYMBOL_FONT).child("⚙")).child(" Ayarlar"))
            // `font: 600 9px mono` — kısayol ölçeklenmez (font kısayolu kök ölçeğini atlar).
            .child(div().font_family(p2_ui::tokens::FONT_MONO).text_size(px(9.)).css_weight(600.).text_color(p.text_4).child("Ctrl + ,"))
            .on_click(cx.listener(|this, _, window, cx| this.open_settings(Section::Account, window, cx)));
        let (danger, white) = (p.danger, gpui_kit::white());
        let logout = div()
            .id("logout")
            .w_full()
            .h(px(33.))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(p.danger_border)
            .rounded(px(8.))
            .bg(p.danger_soft)
            .text_color(p.danger_text)
            .text_size(fs(11.))
            .css_weight(650.)
            .cursor_pointer()
            .hover(move |s| s.bg(danger).border_color(danger).text_color(white))
            .child("Çıkış yap")
            .on_click(cx.listener(|this, _, window, cx| {
                this.user_menu = false;
                let app = this.app.clone();
                let _ = app.update(cx, |app, cx| app.logout(window, cx));
            }));

        let face: AnyElement = match crate::avatars::image(&user.id) {
            Some(image) => gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().rounded(px(12.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
            None => user_initials(&user.display_name).into_any_element(),
        };
        let card = div()
            .id("rail-profile-card")
            .absolute()
            .left(px(78.))
            .bottom(px(12.))
            .w(px(232.))
            .p(px(12.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(14.))
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pb(px(11.))
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(
                        div()
                            .flex_none()
                            .size(px(40.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(13.))
                            .border_1()
                            .border_color(p.border_subtle)
                            .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
                            .text_color(hue.fg)
                            .text_size(fs(12.))
                            .css_weight(800.)
                            .tracking(-0.02)
                            .child(face),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().truncate().text_size(fs(12.)).line_height(px(12.)).css_weight(700.).text_color(p.text_strong).child(user.display_name.clone()))
                            .child(div().text_size(fs(10.)).line_height(px(10.)).text_color(p.text_3).child(format!("@{}", user.username))),
                    ),
            )
            .child(
                div()
                    .mt(px(10.))
                    .mb(px(11.))
                    .h(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(fs(10.))
                    .css_weight(650.)
                    .text_color(p.text_2)
                    .child(presence_flat(&shown, 9., p))
                    .child(shown_label),
            )
            .child(list)
            .children(admin)
            .child(settings)
            .child(logout)
            .child(stack);
        // `p2-scale-in var(--m-base) var(--ease-pop)`, köken sol-alt: ölçek yerine aşağıdan 6px kayma.
        let card = card.with_animation("rail-profile-in", Animation::new(M_BASE), |el, t| {
            let k = EASE_POP.ease(t);
            el.opacity((t * 1.6).min(1.)).bottom(px(12. - 6. * (1. - k)))
        });
        Some(
            div()
                .id("rail-profile-layer")
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.user_menu = false;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }

    /// Profil paneli (ProfilePanel.tsx): sağda sabit çekmece (sohbetin üstünde, daraltmaz);
    /// dışarı basınca, × ya da Esc ile kapanır; sol kenarından genişlik sürüklenir (280–580).
    pub(super) fn profile_panel(&self, member: &Member, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let me = member.id == self.auth.user.id;
        let role = role_label(&member.role);
        let width = crate::resize::value(&crate::resize::PROFILE);
        let hue = p.hues[avatar_hue(&member.id)];
        // Eylemler yalnız kanal sohbetinde (ChannelContent.tsx): "Mesaj gönder" (kendim değilsem) + "@ ile etiketle".
        let in_channel = self.chat.as_ref().is_some_and(|c| matches!(c.read(cx).subject, ChatSubject::Channel(_)));
        let (soft, strong, bg_hover, border_strong) = (p.accent_soft, p.text_strong, p.bg_hover, p.border_strong);

        let close = div()
            .id("profile-close")
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .text_size(fs(18.))
            .text_color(p.text_3)
            .cursor_pointer()
            .hover(move |s| s.bg(soft).text_color(strong))
            .child("×")
            .tooltip(|window, cx| p2_ui::controls::tooltip("Kapat (Esc)", window, cx))
            .on_click(cx.listener(|this, _, _, cx| {
                this.profile = None;
                cx.notify();
            }));
        let header = div()
            .h(px(46.))
            .flex_none()
            .pl(px(16.))
            .pr(px(8.))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(p.border_subtle)
            .child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child("Profil"))
            .child(close);

        // `.profile-portrait`: 150° gradyan bg2 %0 → bg %55 → rgba(0,0,0,.28) %190 (kutuda %100'de ≈ %9 karartma).
        let portrait_face: AnyElement = match crate::avatars::image(&member.id) {
            Some(image) => gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().rounded(px(13.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
            // `.profile-portrait > span`: text-shadow 0 2px 14px rgba(10,20,45,.6)
            None => {
                let (initials, fg) = (user_initials(&member.display_name), hue.fg);
                p2_ui::fx::text_shadow(
                    move |c| div().text_size(fs(46.)).css_weight(800.).tracking(-0.03).text_color(c.unwrap_or(fg)).child(initials.clone()).into_any_element(),
                    0., 2., 14., p2_ui::tokens::css_rgba(10, 20, 45, 0.6),
                )
                .into_any_element()
            }
        };
        let portrait = div()
            .relative()
            .w_full()
            .h(px(196.))
            .flex_none()
            .mb(px(15.))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(14.))
            .bg(linear_gradient(150., linear_color_stop(hue.bg2, 0.), linear_color_stop(hue.bg, 0.55)))
            .shadow({
                let mut s = p.shadow_md.clone();
                s.push(p.inset_top);
                shadows(&s)
            })
            .child(div().absolute().inset_0().bg(linear_gradient(150., linear_color_stop(gpui_kit::black().opacity(0.), 0.55), linear_color_stop(gpui_kit::black().opacity(0.093), 1.))))
            .child(portrait_face)
            .with_animation(("profile-portrait", avatar_hue(&member.id)), Animation::new(M_SLOW), |el, t| {
                let k = EASE_POP.ease(t);
                el.opacity((t * 1.6).min(1.)).top(px(6. * (1. - k)))
            });

        let actions = in_channel.then(|| {
            let (a, b) = (member.clone(), member.username.clone());
            let button = |id: &'static str, label: &'static str| {
                div()
                    .id(id)
                    .flex_1()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .rounded(px(8.))
                    .text_size(fs(11.))
                    .css_weight(650.)
                    .cursor_pointer()
                    .child(label)
            };
            div()
                .flex()
                .gap(px(7.))
                // CSS dikey kenar boşlukları çöker: rol satırının 7px alt boşluğu 14px içinde kalır.
                .mt(px(7.))
                .mb(px(4.))
                .when(!me, |s| {
                    let glow = [p.accent_glow_strong, p.inset_top];
                    s.child(
                        button("profile-dm", "Mesaj gönder")
                            .border_color(gpui_kit::transparent_black())
                            .bg(p.accent_grad.background())
                            .text_color(p.text_on_accent)
                            .shadow(shadows(&glow))
                            .hover(|s| s.opacity(0.96))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.profile = None;
                                this.open_direct(a.clone(), window, cx);
                            })),
                    )
                })
                .child(
                    button("profile-mention", "@ ile etiketle")
                        .border_color(p.border)
                        .bg(p.bg_3)
                        .text_color(p.text)
                        .hover(move |s| s.bg(bg_hover).border_color(border_strong).text_color(strong))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.profile = None;
                            if let Some(chat) = this.chat.clone() {
                                let text = format!("@{b} ");
                                chat.update(cx, |c, cx| c.insert_emoji(&text, window, cx));
                            }
                            cx.notify();
                        })),
                )
        });

        let field = |glyph: &'static str, label: &'static str, value: String, id: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(11.))
                .mb(px(8.))
                .child(
                    div()
                        .flex_none()
                        .size(px(32.))
                        .mr(px(2.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_1()
                        .border_color(p.border)
                        .rounded(px(9.))
                        .bg(p.bg_3)
                        .text_color(p.text_accent)
                        .text_size(fs(14.))
                        .css_weight(750.)
                        .when(glyph != "@", |s| s.font_family(SYMBOL_FONT))
                        .child(glyph),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(div().text_size(fs(11.)).text_color(p.text_3).child(label))
                        .child(div().id(id).truncate().text_size(fs(13.)).text_color(p.link).cursor_pointer().hover(|s| s.underline()).child(value)),
                )
        };
        let username = member.username.clone();
        let email = member.email.clone().filter(|e| !e.trim().is_empty());
        let mut section = div()
            // Çöken kenar boşlukları: önceki öğenin alt boşluğu (eylemler 4, durum/rol satırı 7) 20 içinde.
            .mt(px(if in_channel { 16. } else { 13. }))
            .pt(px(15.))
            .border_t_1()
            .border_color(p.border_subtle)
            .child(div().mb(px(13.)).text_size(fs(14.)).css_weight(700.).text_color(p.text_strong).child("İletişim bilgileri"))
            .child(field("@", "Kullanıcı adı", format!("@{}", member.username), "profile-username").on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(format!("@{username}")));
            }));
        if let Some(mail) = email {
            let url = format!("mailto:{mail}");
            section = section.child(field("✉", "E-posta", mail, "profile-email").on_mouse_down(MouseButton::Left, move |_, _, cx| cx.open_url(&url)));
        }

        let status = member.status.as_str();
        let scroll = div()
            .id("profile-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(16.))
            .px(px(16.))
            .pb(px(22.))
            .flex()
            .flex_col()
            .child(portrait)
            .child(div().mb(px(8.)).text_size(fs(19.)).line_height(px(20.)).css_weight(700.).tracking(-0.01).text_color(p.text_strong).child(member.display_name.clone()))
            .when_some(member.title.clone().filter(|t| !t.is_empty()), |s, t| s.child(div().mt(px(-4.)).mb(px(11.)).text_size(fs(13.)).css_weight(600.).text_color(p.text_3).child(t)))
            .child(
                div()
                    .mb(px(7.))
                    .h(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(fs(11.))
                    .css_weight(600.)
                    .text_color(line_color(status, p))
                    .child(presence_flat(status, 10., p))
                    .child(status_label(status)),
            )
            .when(!role.is_empty(), |s| {
                s.child(
                    div().mb(px(7.)).flex().child(
                        div()
                            .h(px(15.))
                            .px(px(9.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(p.border)
                            .rounded(px(11.))
                            .bg(p.accent_soft)
                            .text_color(p.text_accent)
                            .text_size(fs(9.))
                            .css_weight(750.)
                            .tracking(0.06)
                            .child(match role { "Kurucu" => "KURUCU", "Yönetici" => "YÖNETİCİ", _ => "ÜYE" }),
                    ),
                )
            })
            .children(actions)
            .child(section);

        // `.profile-panel::before`: radial-gradient(120% 100% at 78% 0%, accent-soft 0, rgba(44,123,255,.04) 45%, transparent 70%), 220px.
        let glow = {
            let (rgb, a) = p2_ui::icons::svg_color(p.accent_soft);
            let (w, h) = (width, 220.);
            let markup = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="1" gradientTransform="translate({cx} 0) scale({rx} {ry})"><stop offset="0" stop-color="{rgb}" stop-opacity="{a}"/><stop offset="0.45" stop-color="rgb(44,123,255)" stop-opacity="0.04"/><stop offset="0.7" stop-color="rgb(44,123,255)" stop-opacity="0"/></radialGradient></defs><rect width="{w}" height="{h}" fill="url(#g)"/></svg>"#,
                cx = w * 0.78,
                rx = w * 1.2,
                ry = h,
            );
            p2_ui::icons::svg_image(markup).absolute().top_0().left_0()
        };

        // ProfilePanel.tsx `<ResizeHandle edge="left" label="Profil genişliği"/>`.
        let handle = crate::resize::handle("profile-resize", crate::resize::PROFILE, crate::resize::Edge::Left, "Profil genişliği", &p);

        let panel = div()
            .id("profile-panel")
            .relative()
            .h_full()
            .w(px(width))
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(vec![shadow(-8., 0., 24., 0., css_rgba(0, 0, 0, 0.4), false), shadow(1., 0., 0., 0., css_rgba(255, 255, 255, 0.05), true)])
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(glow)
            .child(header)
            .child(scroll)
            .child(handle);
        // `profile-drawer-in var(--m-slow) var(--ease-elastic)`: 28px sağdan + opaklık.
        let panel = panel.with_animation(("profile-in", avatar_hue(&member.id)), Animation::new(M_SLOW), |el, t| {
            let k = EASE_ELASTIC.ease(t);
            el.opacity(t.min(1.)).left(px(28. * (1. - k)))
        });
        div()
            .id("profile-layer")
            .absolute()
            .inset_0()
            // Panel dışına basınca kapanır (useDismissOutside).
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .bottom_0()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        if !crate::resize::dragging(crate::resize::PROFILE.key) && this.profile.is_some() {
                            this.profile = None;
                            cx.notify();
                        }
                    }))
                    .child(panel),
            )
            .into_any_element()
    }
}

/// Yerel `title` ipucu (sahipli metin).
pub(super) fn tip_owned(text: String) -> impl Fn(&mut gpui_kit::Window, &mut gpui_kit::App) -> gpui_kit::AnyView + 'static {
    move |window, cx| p2_ui::controls::tooltip(text.clone(), window, cx)
}

/// Workspace.tsx `ConfirmDialog`'ları: gruptan çıkar / gruptan ayrıl.
pub(super) enum PeopleConfirm {
    Remove { group: p2_client_core::ChatGroup, user: String, name: String },
    Leave { group_id: String, name: String },
}

impl WorkspaceScreen {
    pub(super) fn ask_remove_member(&mut self, group: p2_client_core::ChatGroup, user: String, name: String, cx: &mut Context<Self>) {
        self.people_confirm = Some(PeopleConfirm::Remove { group, user, name });
        cx.notify();
    }

    pub(super) fn ask_leave_group(&mut self, group_id: String, cx: &mut Context<Self>) {
        let name = self.groups.iter().find(|g| g.id == group_id).map(|g| g.name.clone()).unwrap_or_default();
        self.people_confirm = Some(PeopleConfirm::Leave { group_id, name });
        cx.notify();
    }

    fn run_people_confirm(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        match self.people_confirm.take() {
            Some(PeopleConfirm::Remove { group, user, .. }) => {
                // Üye listesi kapanmış olabilir: değişiklik bu grubun üzerinde yapılır.
                self.group_members = Some(group);
                self.change_group_member(user, false, window, cx);
                self.close_group_members(cx);
            }
            Some(PeopleConfirm::Leave { group_id, .. }) => self.leave_group(group_id, window, cx),
            None => {}
        }
        cx.notify();
    }

    /// ConfirmDialog.tsx (Modal sm): başlık + açıklama, Vazgeç + tehlikeli onay.
    pub(super) fn people_confirm_view(&self, p: &Palette, window: &gpui_kit::Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, body, label) = match self.people_confirm.as_ref()? {
            PeopleConfirm::Remove { group, name, .. } => ("Gruptan çıkar".to_string(), format!("{name} \"{}\" grubundan çıkarılacak. Diğer üyeler bunu görecek.", group.name), "Gruptan çıkar"),
            PeopleConfirm::Leave { name, .. } => ("Gruptan ayrıl".to_string(), format!("\"{name}\" grubundan ayrılacaksın. Diğer üyeler ayrıldığını görecek."), "Gruptan ayrıl"),
        };
        use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, modal, modal_body, modal_head};
        // Kapat düğmesi başlık ızgarasında (alt yazı onun soluna sarılır, Electron gibi).
        let close = button("people-confirm-x", BtnVariant::Ghost, BtnSize::Sm, true, false, p).child("×");
        let cancel = button("people-confirm-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child("Vazgeç");
        let ok = button("people-confirm-yes", BtnVariant::Danger, BtnSize::Md, false, false, p).child(label);
        let dismiss = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut gpui_kit::Window, cx: &mut Context<Self>| {
            this.people_confirm = None;
            cx.notify();
        };
        let card = modal(ModalSize::Sm, window, p)
            .child(modal_head(title, Some(body.into()), Some(close.on_click(cx.listener(dismiss)).into_any_element()), p))
            .child(modal_body())
            .child(p2_ui::controls::modal_foot(p).child(cancel.on_click(cx.listener(dismiss))).child(ok.on_click(cx.listener(|this, _, window, cx| this.run_people_confirm(window, cx)))))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("people-confirm-card", p2_ui::Entrance::PopElastic, std::time::Duration::ZERO);
        Some(
            div()
                .id("people-confirm")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .px(px(24.))
                .pb(px(24.))
                .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.people_confirm = None;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }
}

impl WorkspaceScreen {
    /// Görünüm kancaları (veri yazmaz; grup oluşturma penceresi F'nin `P2_AUTOPILOT_GROUP=@view`):
    /// `P2_AUTOPILOT_PERSON_MENU=<kullanıcı>[@x,y]` kişi sağ tık menüsü (pencere px),
    /// `P2_AUTOPILOT_PEOPLE_CONFIRM=leave:<grup>` / `remove:<grup>:<kullanıcı>` onay penceresi.
    pub(super) fn people_autopilot(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        let env = |k: &str| std::env::var(k).ok();
        let mut retry = false;
        if let Some(spec) = env("P2_AUTOPILOT_PERSON_MENU") {
            let (name, at) = spec.split_once('@').unwrap_or((spec.as_str(), "180,560"));
            let (x, y) = at.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))).unwrap_or((180., 560.));
            match self.members.iter().find(|m| m.username == name).cloned() {
                Some(person) if self.nav_menu.is_none() => {
                    self.nav_menu = Some(super::workspace_nav::NavMenu::Person { person, at: point(px(x), px(y)) });
                    crate::trace(&format!("parite: kişi menüsü @{name}"));
                }
                Some(_) => {}
                None => retry = true,
            }
        }
        if let Some(spec) = env("P2_AUTOPILOT_PEOPLE_CONFIRM").filter(|_| self.people_confirm.is_none()) {
            let mut parts = spec.splitn(3, ':');
            let (verb, group, user) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            match self.groups.iter().find(|g| g.name == group).cloned() {
                Some(g) if verb == "remove" => {
                    if let Some(m) = g.members.iter().find(|m| m.username == user).cloned() {
                        self.people_confirm = Some(PeopleConfirm::Remove { group: g, user: m.id, name: m.display_name });
                    }
                }
                Some(g) => self.people_confirm = Some(PeopleConfirm::Leave { group_id: g.id, name: g.name }),
                None => retry = true,
            }
            if self.people_confirm.is_some() {
                crate::trace("parite: kişi onay penceresi");
            }
        }
        cx.notify();
        if retry {
            static TRIES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            if TRIES.fetch_add(1, Ordering::Relaxed) < 20 {
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(400)).await;
                    let _ = this.update_in(cx, |this, window, cx| this.people_autopilot(window, cx));
                })
                .detach();
            }
        }
    }
}
