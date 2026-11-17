//! Grup üyeleri (GroupChat.tsx üye menüsü): grup adı (yeniden adlandır), üyeler, kişi ekle, çıkar.
//! Proje grubunda eklenecekler proje üyeleridir; Ana sayfa grubunda arkadaşlarım ve proje
//! paylaştığım kişiler (docs/arkadaslar.md).

use gpui_kit::prelude::*;
use p2_ui::P2Fx as _; // backdrop_blur
use gpui_kit::{AnyElement, Context, SharedString, Window, div, px};
use p2_client_core::ChatGroup;
use p2_ui::P2Text as _;
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::tokens::{Palette, fs};
use p2_ui::{Entrance, P2Motion as _};

use super::WorkspaceScreen;
use crate::task;

impl WorkspaceScreen {
    pub(super) fn open_group_members(&mut self, group: ChatGroup, window: &mut Window, cx: &mut Context<Self>) {
        crate::trace(&format!("grup: üyeler açıldı ({} kişi)", group.members.len()));
        let name = group.name.clone();
        self.group_rename.update(cx, |input, cx| input.set_value(name, window, cx));
        self.group_members = Some(group);
        if let Some(chat) = self.chat.clone() {
            chat.update(cx, |c, cx| c.mark_people_menu(cx));
        }
        cx.notify();
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_GROUP_MEMBERS") {
            self.group_steps = spec.split(';').map(str::to_string).rev().collect();
            self.group_step(window, cx);
        }
    }

    fn group_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.group_steps.pop() else { return };
        let Some((verb, name)) = step.split_once(':') else { return };
        let Some(user) = self.members.iter().find(|m| m.username == name).map(|m| m.id.clone()) else {
            // Proje üyeleri henüz yüklenmemiş olabilir: kısa süre sonra yeniden dene.
            if self.members.is_empty() {
                self.group_steps.push(step);
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                    let _ = this.update_in(cx, |this, window, cx| this.group_step(window, cx));
                })
                .detach();
                return;
            }
            return self.group_step(window, cx);
        };
        self.change_group_member(user, verb == "add", window, cx);
    }

    /// Açık grubun adını "Grup adı" alanındaki değerle değiştirir (boş: üyelerden türetilir).
    pub(super) fn rename_open_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let group = self.group_members.clone().or_else(|| match self.current_scope(cx) {
            Some(p2_client_core::Scope::Group(id)) => self.group_by_id(&id).cloned(),
            _ => None,
        });
        let Some(group) = group else { return };
        let name = self.group_rename.read(cx).value().trim().to_string();
        if name == group.name {
            return;
        }
        let (api, org, gid) = (self.api.clone(), self.org.id.clone(), group.id.clone());
        task::run(window, cx, move || api.rename_group(&org, &gid, &name), move |this, r, window, cx| match r {
            Ok(updated) => {
                crate::trace(&format!("grup: ad «{}»", updated.name));
                for g in this.groups.iter_mut().chain(this.home.groups.iter_mut()).filter(|g| g.id == updated.id) {
                    g.name = updated.name.clone();
                }
                if this.group_members.as_ref().is_some_and(|g| g.id == updated.id) {
                    let name = updated.name.clone();
                    this.group_rename.update(cx, |input, cx| input.set_value(name, window, cx));
                    this.group_members = Some(updated.clone());
                }
                if let Some(chat) = &this.chat {
                    chat.update(cx, |c, cx| c.set_group(updated, cx));
                }
                cx.notify();
            }
            Err(e) => this.fail(e, window, cx),
        });
    }

    pub(super) fn change_group_member(&mut self, user: String, add: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.group_members.clone() else { return };
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let gid = group.id.clone();
        let removed = user.clone();
        task::run(
            window,
            cx,
            move || if add { api.add_group_members(&org, &gid, std::slice::from_ref(&user)).map(Some) } else { api.remove_group_member(&org, &gid, &user).map(|_| None) },
            move |this, r, window, cx| match r {
                Ok(updated) => {
                    crate::trace(&format!("grup: üye {}", if add { "eklendi" } else { "çıkarıldı" }));
                    this.load_groups(window, cx);
                    this.load_home_groups(window, cx);
                    if let Some(g) = updated {
                        this.group_members = Some(g.clone());
                        if let Some(chat) = &this.chat {
                            chat.update(cx, |c, cx| c.set_group(g, cx));
                        }
                    } else if let Some(g) = this.group_members.as_mut() {
                        g.members.retain(|m| m.id != removed);
                    }
                    cx.notify();
                    this.group_step(window, cx);
                }
                Err(e) => this.fail(e, window, cx),
            },
        );
    }

    /// GroupChat.tsx `.group-add-menu`: başlıktaki "Üyeler" düğmesinin altında (sağ kenar hizalı,
    /// 6px boşluk) 250px açılır liste — üyeler (kurucu × ile çıkarır) ve "Gruba ekle".
    pub(super) fn group_members_modal(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let group = self.group_members.as_ref()?;
        let me = self.auth.user.id.clone();
        let owner = group.created_by_id.as_deref() == Some(me.as_str());
        let (soft, strong, bg_hover, danger_soft, danger_text) = (p.accent_soft, p.text_strong, p.bg_hover, p.danger_soft, p.danger_text);
        let renamed = self.group_rename.read(cx).value().trim() != group.name;
        // Başlıktaki `.channel-content > header p` kuralı (0,1,2) `.group-add-menu > p`yi ezer: renk text-3,
        // 11px, kenar boşluğu 0; yalnız ikinci başlıkta `:not(:first-child)` üst boşluk + çizgi.
        let label = |text: String, first: bool| {
            div()
                .when(!first, |s| s.mt(px(8.)).pt(px(8.)).border_t_1().border_color(p.border_subtle))
                .text_size(fs(11.))
                .css_weight(800.)
                .tracking(0.12)
                .text_color(p.text_3)
                .child(text)
        };
        let small_avatar = |id: &str, name: &str| {
            let hue = p.hues[avatar_hue(id)];
            div()
                .flex_none()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                .text_color(hue.fg)
                .text_size(fs(8.))
                .line_height(px(20.))
                .css_weight(800.)
                .tracking(-0.02)
                .child(p2_ui::components::initials_label(user_initials(name)))
        };
        let mut menu = div()
            .id("group-add-menu")
            .w(px(250.))
            .max_h(px(330.))
            .overflow_y_scroll()
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(11.))
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // Grup adı: her üye değiştirebilir (boş bırakılırsa üyelerin adlarından oluşur).
            .child(label(p2_i18n::t("dm.group.name"), true))
            .child(
                div()
                    .flex_none()
                    .mt(px(4.))
                    .mb(px(2.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(30.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.bg_3)
                            .child(gpui_kit::component::input::Input::new(&self.group_rename).appearance(false).w_full().h(px(28.)).px_0().py_0().text_size(fs(11.5))),
                    )
                    .child(
                        div()
                            .id("gm-rename")
                            .flex_none()
                            .h(px(30.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .border_1()
                            .border_color(p.border_accent)
                            .bg(p.accent_soft_2)
                            .text_size(fs(10.5))
                            .css_weight(700.)
                            .text_color(p.text_strong)
                            .when(renamed, |s| s.cursor_pointer())
                            .when(!renamed, |s| s.opacity(0.5))
                            .child(p2_i18n::t("dm.group.save"))
                            .on_click(cx.listener(|this, _, window, cx| this.rename_open_group(window, cx))),
                    ),
            )
            .child(label(p2_i18n::tv("grp.members", &[("n", &group.members.len().to_string())]), false));
        for m in &group.members {
            let row_id: SharedString = format!("gm-row-{}", m.id).into();
            let (id, name) = (m.id.clone(), m.display_name.clone());
            menu = menu.child(
                div()
                    .id(row_id.clone())
                    .group(row_id.clone())
                    .flex_none()
                    .min_h(px(30.))
                    .pl(px(7.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .text_size(fs(11.5))
                    .text_color(p.text)
                    .hover(move |s| s.bg(bg_hover))
                    .child(small_avatar(&m.id, &m.display_name))
                    .child(div().flex_1().min_w_0().truncate().css_weight(550.).child(m.display_name.clone()))
                    .when(m.id == me, |s| s.child(div().flex_none().py(px(6.)).px(px(4.)).text_size(fs(8.5)).css_weight(750.).tracking(0.06).text_color(p.text_accent).child(p2_i18n::t("grp.youTag"))))
                    .when(m.id != me && owner, |s| {
                        s.child(
                            div()
                                .id(SharedString::from(format!("gm-kick-{}", m.id)))
                                .flex_none()
                                .size(px(20.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .text_size(fs(14.))
                                .line_height(px(14.))
                                .text_color(p.text_3)
                                .invisible()
                                .group_hover(row_id.clone(), |s| s.visible())
                                .cursor_pointer()
                                .hover(move |s| s.bg(danger_soft).text_color(danger_text))
                                .child("×")
                                .tooltip(super::people_ui::tip_owned(p2_i18n::tv("grp.remove", &[("name", &name)])))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let Some(g) = this.group_members.clone() else { return };
                                    this.ask_remove_member(g, id.clone(), name.clone(), cx);
                                })),
                        )
                    }),
            );
        }
        menu = menu.child(label(p2_i18n::t("grp.addToGroup"), false));
        // Proje grubu: proje üyeleri. Ana sayfa grubu: arkadaşlarım ve proje paylaştığım kişiler.
        let pool: Vec<p2_client_core::Member> = if group.home { self.home_group_pool() } else { self.members.clone() };
        let missing: Vec<_> = pool.iter().filter(|m| m.id != me && !group.members.iter().any(|g| g.id == m.id)).cloned().collect();
        for m in &missing {
            let id = m.id.clone();
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("gm-add-{}", m.id)))
                    .h(px(30.))
                    .flex_none()
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(6.))
                    .text_size(fs(11.5))
                    .text_color(p.text)
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft).text_color(strong))
                    .child(small_avatar(&m.id, &m.display_name))
                    .child(m.display_name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.change_group_member(id.clone(), true, window, cx);
                        this.close_group_members(cx);
                    })),
            );
        }
        if missing.is_empty() {
            menu = menu.child(div().py(px(6.)).px(px(4.)).text_size(fs(10.5)).text_color(p.text_3).child(p2_i18n::t("grp.noOneToAdd")));
        }
        // Düğme: başlık sağ boşluğu 18 + ••• ve ara düğmeleri (32 + 2 aralık) → sağ kenar = genişlik − 86.
        let vw = f32::from(window.viewport_size().width);
        let right = vw - 86.;
        Some(
            div()
                .absolute()
                .top(px(88.5 - 38.))
                .left(px(right - 250.))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_group_members(cx)))
                .child(menu.p2_enter("group-add-menu", Entrance::ScaleIn, std::time::Duration::ZERO))
                .into_any_element(),
        )
    }

    /// Açılır listeyi kapatır; sohbet başlığındaki "Üyeler" düğmesinin etkin görünümü de söner.
    pub(super) fn close_group_members(&mut self, cx: &mut Context<Self>) {
        if self.group_members.take().is_some() {
            if let Some(chat) = self.chat.clone() {
                chat.update(cx, |c, cx| c.close_people_menu(cx));
            }
            cx.notify();
        }
    }
}
