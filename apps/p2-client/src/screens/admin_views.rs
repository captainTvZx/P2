//! Yönetim paneli çizimi (admin.rs alt modülü): bölüm görünümleri, onay pencereleri, kabuk.

use super::*;
use gpui_kit::MouseButton;
use gpui_kit::component::input::Input;
use p2_ui::controls::{menu, menu_item, modal_foot};
use p2_ui::form::native_checkbox;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::FONT_MONO;
use p2_ui::{Entrance, P2Motion as _};

use super::super::workspace::status_label;

// ---------------------------------------------------------------- çizim

/// `.admin-stat`: p 12 13, kenarlık `border`, r 11, `glass`, inset-top; small (fs-2xs 800, .12em,
/// büyük harf, accent), b (20px 750, −.02em), span (10px text-3, 1.4). İpucu yoksa ızgara satırları
/// esnetilir (Electron'da hücre yüksekliği satırın en yükseğine uzar).
fn stat(label: &str, value: String, hint_text: Option<String>, p: &Palette) -> Div {
    let has_hint = hint_text.is_some();
    let grow = |d: Div| if has_hint { d } else { d.flex_grow(1.) };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.))
        .px(px(13.))
        .py(px(12.))
        .border_1()
        .border_color(p.border)
        .rounded(radius::MD)
        .bg(p.glass.background())
        .shadow(shadows(&[p.inset_top]))
        .child(grow(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(label))))
        .child(grow(text(fs(20.)).css_weight(750.).tracking(-0.02).text_color(p.text_strong).child(value)))
        .when_some(hint_text, |s, h| s.child(div().text_size(fs(10.)).line_height(gpui_kit::relative(1.4)).text_color(p.text_3).child(h)))
}

/// `.admin-stats`: `repeat(auto-fit, minmax(150px, 1fr))` gap 8.
fn stats_grid(items: Vec<Div>, width: f32) -> Div {
    let cols = (((width + 8.) / 158.).floor() as usize).max(1).min(items.len().max(1));
    let mut col = div().flex().flex_col().gap(px(8.));
    let mut row: Vec<AnyElement> = Vec::new();
    let n = items.len();
    for (i, item) in items.into_iter().enumerate() {
        row.push(div().flex_1().min_w_0().flex().child(item.p2_enter(SharedString::from(format!("stat-{width}-{i}-{n}")), Entrance::Rise, std::time::Duration::ZERO)).into_any_element());
        if row.len() == cols || i + 1 == n {
            while row.len() < cols {
                row.push(div().flex_1().into_any_element());
            }
            col = col.child(div().flex().gap(px(8.)).children(std::mem::take(&mut row)));
        }
    }
    col
}

/// AdminTeam.tsx `Tag` (`.team-tag`): `@handle`, renk verilmişse zemin/kenarlık/yazı o renkten.
fn tag(handle: &str, color: Option<&str>, p: &Palette) -> Div {
    let c = color.map(|c| hex(Some(c), p.text));
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .px(px(9.))
        .py(px(4.))
        .border_1()
        .rounded(radius::PILL)
        .font_family(FONT_MONO)
        .text_size(fs(10.5))
        .line_height(lh(fs(10.5)))
        .css_weight(650.)
        .map(|s| match c {
            Some(c) => s.bg(c.opacity(0x22 as f32 / 255.)).text_color(c).border_color(c.opacity(0x55 as f32 / 255.)),
            None => s.bg(p.bg_3).text_color(p.text).border_color(p.border),
        })
        .child(format!("@{handle}"))
}

/// `.team-tag button`: 14px yuvarlak ×, hover kırmızı.
fn tag_remove(id: impl Into<ElementId>, p: &Palette) -> Stateful<Div> {
    let danger = p.danger;
    div()
        .id(id)
        .size(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.bg_hover)
        .font_family(p2_ui::tokens::FONT)
        .text_size(fs(11.))
        .line_height(px(14.))
        .cursor_pointer()
        .hover(move |s| s.bg(danger).text_color(gpui_kit::white()))
        .child("×")
}

/// `.role-colors button`: 22px yuvarlak; etkin `text-strong` 2px kenarlık + 2px `bg-2` halka.
fn color_dot(id: impl Into<ElementId>, color: &str, active: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(22.))
        .flex_none()
        .rounded_full()
        .border_2()
        .bg(hex(Some(color), p.accent))
        .cursor_pointer()
        .map(|s| if active { s.border_color(p.text_strong).shadow(shadows(&[Shadow::new(0., 0., 0., 2., p.bg_2)])) } else { s.border_color(gpui_kit::transparent_black()) })
}

impl AdminPanel {
    fn overview_view(&self, width: f32, p: &Palette) -> AnyElement {
        let Some(o) = &self.overview else { return hint("Yükleniyor…", p).into_any_element() };
        let pname = self.project().map(|p| p.name.clone()).unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .child(settings_block(format!("{pname} topluluğu"), false, p).child(stats_grid(
                vec![
                    stat("Toplam üye", o.members.total.to_string(), Some(format!("{} kişi çevrimiçi", o.members.online)), p),
                    stat("Rol", o.members.roles.to_string(), Some(format!("{} proje sahibi", o.members.owners)), p),
                    stat("Bu hafta katılan", o.members.joined_this_week.to_string(), None, p),
                    stat("2FA açık", format!("{}/{}", o.members.two_factor, o.members.total), Some(if o.members.two_factor == o.members.total { "Herkes korumalı".into() } else { "Kalanları teşvik et".into() }), p),
                ],
                width,
            )))
            .child(settings_block("İçerik", false, p).child(stats_grid(
                vec![
                    stat("Mesaj", o.content.messages.to_string(), Some(format!("{} kanal · {} bireysel · {} grup", o.content.channel_messages, o.content.direct_messages, o.content.group_messages)), p),
                    stat("Kanal", o.content.channels.to_string(), Some(if o.content.deleted_channels > 0 { format!("{} silinmiş", o.content.deleted_channels) } else { "Silinmiş kanal yok".into() }), p),
                    stat("Sohbet grubu", o.content.groups.to_string(), None, p),
                ],
                width,
            )))
            .child(settings_block("Sistem", true, p).child(stats_grid(
                vec![
                    stat("Dosya alanı", bytes(o.storage.bytes), Some(format!("{} dosya", o.storage.files)), p),
                    stat("Açık oturum", o.sessions.to_string(), None, p),
                    stat("P2 Server", format!("v{}", o.server.version), None, p),
                    stat("Etkin davet kodu", o.invites.to_string(), None, p),
                ],
                width,
            )))
            .into_any_element()
    }

    /// AdminTeam.tsx `Picker` (`.team-picker`, yerel `<select>`): kesikli hap; tıklayınca seçenek menüsü.
    fn picker_view(&self, which: Picker, label: &str, empty: &str, options: Vec<(String, String)>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        if options.is_empty() {
            return div().text_size(fs(10.)).line_height(lh(fs(10.))).italic().text_color(p.text_3).child(empty.to_string()).into_any_element();
        }
        let open = self.picker.as_ref() == Some(&which);
        let w = which.clone();
        let mut el = div()
            .relative()
            .child(
                div()
                    .id(SharedString::from(format!("picker-{label}")))
                    .h(px(26.))
                    .pl(px(8.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .border_1()
                    .border_dashed()
                    .border_color(p.border_accent)
                    .rounded(radius::PILL)
                    .text_color(p.text_accent)
                    .text_size(fs(10.5))
                    .line_height(lh(fs(10.5)))
                    .cursor_pointer()
                    .child(label.to_string())
                    .child(p2_ui::glyphs::ui_stroke("chevron-down", 12., 2.).text_color(p.text_accent))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.picker = if this.picker.as_ref() == Some(&w) { None } else { Some(w.clone()) };
                        cx.notify();
                    })),
            );
        if open {
            let mut list = menu("picker-menu", p).absolute().top(px(30.)).left_0().max_h(px(260.)).overflow_y_scroll();
            for (id, name) in options {
                let which = which.clone();
                list = list.child(menu_item(SharedString::from(format!("pick-{id}")), name, None, None, false, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.picker = None;
                    match &which {
                        Picker::GiveRole(user) => this.assign(user.clone(), id.clone(), true, window, cx),
                        Picker::GivePerson(role) => this.assign(id.clone(), role.clone(), true, window, cx),
                        Picker::AddToGroup(group) => this.group_member(group.clone(), id.clone(), true, window, cx),
                        Picker::LinkRole(group) => this.link_group(id.clone(), group.clone(), true, window, cx),
                        Picker::LinkGroup(role) => this.link_group(role.clone(), id.clone(), true, window, cx),
                        Picker::DefaultChannel => {
                            this.draft_default = (!id.is_empty()).then(|| id.clone());
                            cx.notify();
                        }
                    }
                })));
            }
            el = el.child(gpui_kit::deferred(list.p2_enter("picker-menu", Entrance::ScaleIn, std::time::Duration::ZERO)).with_priority(2));
        }
        el.into_any_element()
    }

    /// `.team-head`: ızgara `auto 1fr auto`, p 11 13, kenarlık, r 11, `glass`, inset-top.
    fn team_head(lead: AnyElement, title: AnyElement, lines: Vec<String>, actions: Div, p: &Palette) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(13.))
            .py(px(11.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.glass.background())
            .shadow(shadows(&[p.inset_top]))
            .child(lead)
            .child(div().flex_1().min_w_0().flex().flex_col().child(div().mb(px(2.)).child(title)).children(lines.into_iter().map(|l| div().text_size(fs(10.)).line_height(gpui_kit::relative(1.45)).text_color(p.text_3).child(l))))
            .child(actions.max_w(px(320.)).flex().flex_wrap().items_center().justify_end().gap(px(6.)))
    }

    fn team_view(&self, width: f32, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can_roles = self.perms.can("member.roles");
        let can_groups = self.perms.can("usergroup.manage");
        let can_kick = self.perms.can("member.kick");
        let primary = self.primary_owner();
        let needle = self.search.read(cx).value().trim().to_lowercase();
        let has = |v: &str| needle.is_empty() || v.to_lowercase().contains(&needle);

        // `.team-tabs > button`: h 32, p 0 14, hap, kenarlık; etkin accent-soft + text-accent; em sayaç.
        // Üyeler / Roller / Gruplar ayrı panel bölümleridir; haplar bölümü değiştirir.
        let tab = |id: &'static str, label: &str, count: usize, on: bool, section: Section| {
            let (bs, strong) = (p.border_strong, p.text_strong);
            div()
                .id(id)
                .h(px(32.))
                .px(px(14.))
                .flex()
                .items_center()
                .gap(px(6.))
                .border_1()
                .rounded(radius::PILL)
                .text_size(fs(11.5))
                .line_height(lh(fs(11.5)))
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_accent) } else { s.border_color(p.border).text_color(p.text_3).hover(move |s| s.border_color(bs).text_color(strong)) })
                .child(label.to_string())
                .child(div().px(px(6.)).py(px(1.)).rounded(radius::PILL).bg(if on { p.accent_soft_2 } else { p.bg_hover }).text_size(fs(9.5)).line_height(lh(fs(9.5))).child(count.to_string()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.section != section {
                        this.open(section, window, cx);
                    }
                }))
        };
        let soft = p.accent_soft;
        let can_create = match self.tab {
            TeamTab::Roles => can_roles,
            TeamTab::Groups => can_groups,
            TeamTab::Members => false,
        };
        let tabs = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(tab("tt-members", "Üyeler", self.members.len(), self.tab == TeamTab::Members, Section::Members))
            .child(tab("tt-roles", "Roller", self.roles.len(), self.tab == TeamTab::Roles, Section::Roles))
            .child(tab("tt-groups", "Gruplar", self.groups.len(), self.tab == TeamTab::Groups, Section::Groups))
            .when(can_create, |s| {
                s.child(
                    div()
                        .id("team-create")
                        .ml_auto()
                        .h(px(32.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .border_1()
                        .border_dashed()
                        .border_color(p.border_accent)
                        .rounded(radius::PILL)
                        .text_color(p.text_accent)
                        .text_size(fs(11.5))
                        .line_height(lh(fs(11.5)))
                        .css_weight(650.)
                        .cursor_pointer()
                        .hover(move |s| s.bg(soft))
                        .child(if self.tab == TeamTab::Groups { "＋ Yeni grup" } else { "＋ Yeni rol" })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.color = ROLE_COLORS[0];
                            this.role_name.update(cx, |s, cx| s.set_value("", window, cx));
                            this.role_desc.update(cx, |s, cx| s.set_value("", window, cx));
                            this.creating = true;
                            cx.notify();
                        })),
                )
            });

        // Sol liste (`.team-list`, 250px): arama + satırlar.
        let search = div()
            .h(px(32.))
            .flex_none()
            .flex()
            .items_center()
            .px(px(8.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::SM)
            .bg(p.bg_3)
            .child(div().w(px(22.)).flex_none().text_color(p.text_4).child("⌕"))
            .child(Input::new(&self.search).appearance(false).bordered(false).focus_bordered(false).text_size(fs(12.)).text_color(p.text).px_0().h(px(30.)));
        let mut list = div().id("team-list").w(px(250.)).flex_none().max_h(px(f32::from(window.viewport_size().height) * 0.58)).overflow_y_scroll().pr(px(4.)).flex().flex_col().gap(px(3.)).child(search);
        let member_list: Vec<&TeamMember> = self.members.iter().filter(|m| has(&m.display_name) || has(&m.username) || has(&m.email)).collect();
        let role_list: Vec<&TeamRole> = self.roles.iter().filter(|r| has(&r.name) || has(&r.handle)).collect();
        let group_list: Vec<&ProjectUserGroup> = self.groups.iter().filter(|g| has(&g.name) || has(&g.handle)).collect();
        let member = self.members.iter().find(|m| Some(&m.id) == self.selected_member.as_ref()).or_else(|| member_list.first().copied()).cloned();
        let role = self.roles.iter().find(|r| Some(&r.id) == self.selected_role.as_ref()).or_else(|| role_list.first().copied()).cloned();
        let group = self.groups.iter().find(|g| Some(&g.id) == self.selected_group.as_ref()).or_else(|| group_list.first().copied()).cloned();
        let row_base = |id: String, active: bool| {
            let hover = p.bg_hover;
            div()
                .id(SharedString::from(id))
                .relative()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(9.))
                .px(px(9.))
                .py(px(7.))
                .border_1()
                .rounded(radius::MD)
                .cursor_pointer()
                .map(|s| if active { s.border_color(p.border_accent).bg(p.accent_soft) } else { s.border_color(gpui_kit::transparent_black()).hover(move |s| s.bg(hover).left(px(2.))) })
        };
        let name_col = |name: String, sub: String| {
            div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).text_color(p.text).truncate().child(name)).child(text(fs(10.)).text_color(p.text_3).truncate().child(sub))
        };
        match self.tab {
            TeamTab::Members => {
                for m in &member_list {
                    let id = m.id.clone();
                    let sub = if m.primary_owner { "Proje sahibi".to_string() } else if m.owner { "Sahip".into() } else if m.roles.is_empty() { "rol yok".into() } else { m.roles.iter().map(|r| r.name.clone()).collect::<Vec<_>>().join(", ") };
                    list = list.child(
                        row_base(format!("tm-{}", m.id), member.as_ref().is_some_and(|x| x.id == m.id))
                            .child(div().relative().flex_none().child(admin_avatar(&m.id, &m.display_name, true, p)).child(presence(&m.status, p)))
                            .child(name_col(m.display_name.clone(), sub))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_member = Some(id.clone());
                                this.picker = None;
                                cx.notify();
                            })),
                    );
                }
            }
            TeamTab::Roles => {
                for r in &role_list {
                    let id = r.id.clone();
                    let sub = if r.founder { "Proje sahipleri".to_string() } else if r.everyone { "Projedeki herkes".into() } else { format!("{} kişi · {} grup", r.members.len(), r.groups.len()) };
                    list = list.child(
                        row_base(format!("tr-{}", r.id), role.as_ref().is_some_and(|x| x.id == r.id))
                            .child(div().size(px(10.)).flex_none().rounded_full().bg(hex(r.color.as_deref(), p.neutral_fallback)))
                            .child(name_col(r.name.clone(), format!("{sub} · {} yetki", r.permissions.len())))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_role = Some(id.clone());
                                this.picker = None;
                                cx.notify();
                            })),
                    );
                }
                if role_list.is_empty() {
                    list = list.child(hint("Rol yok.", p));
                }
            }
            TeamTab::Groups => {
                for g in &group_list {
                    let id = g.id.clone();
                    let linked = self.roles.iter().filter(|r| r.groups.iter().any(|x| x.id == g.id)).count();
                    list = list.child(
                        row_base(format!("tg-{}", g.id), group.as_ref().is_some_and(|x| x.id == g.id))
                            .child(div().size(px(10.)).flex_none().rounded_full().bg(hex(g.color.as_deref(), p.neutral_fallback)))
                            .child(name_col(g.name.clone(), format!("@{} · {} kişi · {} rol", g.handle, g.members.len(), linked)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_group = Some(id.clone());
                                this.picker = None;
                                cx.notify();
                            })),
                    );
                }
                if group_list.is_empty() {
                    list = list.child(hint("Henüz grup yok. Yukarıdan bir grup oluştur.", p));
                }
            }
        }

        // Sağ ayrıntı (`.team-detail`)
        let detail_w = width - 250. - 14.;
        let mut detail = div().flex_1().min_w_0().flex().flex_col().gap(px(10.));
        let chips = || div().flex().flex_wrap().items_center().gap(px(6.));
        let small = |t: &str| div().text_size(fs(10.5)).line_height(lh(fs(10.5))).text_color(p.text_3).child(t.to_string());
        // `.team-detail .settings-block`: p 0, kenarlık yok (yalnız gap 9).
        let block = |title: &str| div().flex().flex_col().gap(px(9.)).child(block_title(title, p));
        if self.creating {
            let groups_tab = self.tab == TeamTab::Groups;
            let mut colors = div().flex().flex_wrap().gap(px(6.));
            for c in ROLE_COLORS {
                colors = colors.child(color_dot(SharedString::from(format!("nc-{c}")), c, self.color == c, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.color = c;
                    cx.notify();
                })));
            }
            let name = self.role_name.read(cx).value().trim().to_string();
            let name_ok = name.chars().count() >= 2 && p2_client_core::role_handle(&name).len() >= 2;
            let fw = (detail_w - 26. - 18. - 150.) / 2.3;
            let second = if groups_tab {
                settings_field("Etiket", div().h(px(34.)).flex().items_center().px(px(10.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_1).font_family(FONT_MONO).text_size(fs(12.)).text_color(p.text_3).child(format!("@{}", if name_ok { p2_client_core::role_handle(&name) } else { "etiket".into() })), p)
            } else {
                settings_field("Açıklama", field_input(&self.role_desc, false, p, window, cx), p)
            };
            detail = detail.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .px(px(13.))
                    .py(px(12.))
                    .border_1()
                    .border_color(p.border_accent)
                    .rounded(radius::MD)
                    .bg(p.accent_soft)
                    .child(block_title(if groups_tab { "Yeni grup" } else { "Yeni rol" }, p))
                    .child(div().flex().gap(px(9.)).child(div().w(px(150.)).flex_none().child(settings_field(if groups_tab { "Grup adı" } else { "Rol adı" }, field_input(&self.role_name, false, p, window, cx), p))).child(div().w(px(fw * 1.3)).min_w_0().child(second)))
                    .when(groups_tab, |s| s.child(hint("Kanalda @etiket yazınca gruptaki herkes bildirim alır; gruba rol bağlayarak yetki de verebilirsin.", p)))
                    .child(settings_field("Renk", colors, p))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(settings_button("role-create", "Oluştur", Btn::Primary, self.busy || !name_ok, p).on_click(cx.listener(move |this, _, window, cx| {
                                if name_ok {
                                    this.create_role(window, cx)
                                }
                            })))
                            .child(settings_button("role-cancel", "Vazgeç", Btn::Plain, false, p).on_click(cx.listener(|this, _, _, cx| {
                                this.creating = false;
                                cx.notify();
                            }))),
                    ),
            );
        } else if self.tab == TeamTab::Members {
            if let Some(m) = member {
                let mut actions = div();
                let protected = m.primary_owner || m.id == self.me;
                if primary && !m.primary_owner {
                    let b = m.clone();
                    actions = actions.child(settings_button("m-transfer", "Sahipliği devret", Btn::Plain, self.busy, p).h(px(28.)).on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(Confirm::Transfer(b.clone()));
                        cx.notify();
                    })));
                }
                if can_kick {
                    let c = m.clone();
                    let disabled = self.busy || protected;
                    actions = actions.child(settings_button("m-remove", "Çıkar", Btn::Plain, disabled, p).h(px(28.)).on_click(cx.listener(move |this, _, _, cx| {
                        if !disabled {
                            this.confirm = Some(Confirm::Remove(c.clone()));
                            cx.notify();
                        }
                    })));
                    let d = m.clone();
                    actions = actions.child(settings_button("m-ban", "Yasakla", Btn::Danger, disabled, p).h(px(28.)).on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            this.ask_ban(d.clone(), window, cx);
                        }
                    })));
                }
                let title = div()
                    .flex()
                    .items_baseline()
                    .child(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(m.display_name.clone()))
                    .when(m.id == self.me, |s| s.child(text(fs(11.)).css_weight(500.).text_color(p.text_3).child(" · sen")))
                    .when(m.primary_owner, |s| s.child(div().ml(px(8.)).px(px(8.)).py(px(2.)).border_1().border_color(css_rgba(255, 181, 69, 0.5)).rounded(radius::PILL).bg(p.warning_soft).text_color(p.warning_text).text_size(fs(9.)).line_height(lh(fs(9.))).font_weight(FontWeight::BOLD).child("proje sahibi")));
                let seen = m.last_client_seen_at.as_deref().map(|s| format!(" · {}", stamp(s))).unwrap_or_default();
                let lines = vec![
                    format!("@{} · {}", m.username, m.email),
                    format!("{} · {} mesaj · {} katıldı · {}", status_label(&m.status), m.channel_messages, day(&m.joined_at), if m.two_factor_enabled { "2FA açık" } else { "2FA kapalı" }),
                    format!("Son P2 Client: {}{seen}", m.last_client_version.as_ref().map(|v| format!("v{v}")).unwrap_or_else(|| "bilinmiyor".into())),
                ];
                detail = detail.child(Self::team_head(div().relative().flex_none().child(admin_avatar(&m.id, &m.display_name, false, p)).child(presence(&m.status, p)).into_any_element(), title.into_any_element(), lines, actions, p));
                let mut row = chips();
                for r in &m.roles {
                    let removable = can_roles && !r.via_group && !r.founder;
                    let (uid, rid) = (m.id.clone(), r.id.clone());
                    let mut t = tag(&r.handle, r.color.as_deref(), p);
                    if removable {
                        t = t.child(tag_remove(SharedString::from(format!("rx-{}", r.id)), p).on_click(cx.listener(move |this, _, window, cx| this.assign(uid.clone(), rid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if m.roles.is_empty() {
                    row = row.child(small("Bu kişide rol yok."));
                }
                if can_roles {
                    let options: Vec<(String, String)> = self.roles.iter().filter(|r| !r.everyone && !r.founder && !m.roles.iter().any(|x| x.id == r.id)).map(|r| (r.id.clone(), r.name.clone())).collect();
                    let empty = if self.roles.iter().any(|r| !r.everyone && !r.founder) { "Tüm roller verilmiş" } else { "Önce Roller sekmesinden bir rol oluştur" };
                    row = row.child(self.picker_view(Picker::GiveRole(m.id.clone()), "＋ rol ver", empty, options, p, cx));
                }
                detail = detail.child(block("Roller").child(row));
                if !m.groups.is_empty() {
                    let mut row = chips();
                    for g in &m.groups {
                        row = row.child(tag(&g.handle, g.color.as_deref(), p));
                    }
                    detail = detail.child(block("Gruplar").child(row));
                }
                let custom = self.overrides.get(&m.id).map_or(0, |o| o.allow.len() + o.deny.len());
                if can_roles && !m.owner && custom > 0 {
                    let id = m.id.clone();
                    detail = detail.child(block("Kişiye özel yetkiler").child(hint(format!("Bu kişide rollerden bağımsız {custom} özel yetki ayarı var. İzinler bölümünden düzenleyebilir ya da hepsini temizleyebilirsin."), p)).child(div().flex().child(settings_button("m-reset", "Özel ayarları temizle", Btn::Plain, self.busy, p).on_click(cx.listener(move |this, _, window, cx| {
                        let id = id.clone();
                        let label = "Özel ayarlar temizlendi.".to_string();
                        this.team_action(label.clone(), label, move |api, pid| api.set_member_permissions(pid, &id, &[], &[]), window, cx);
                    })))));
                }
            }
        } else if self.tab == TeamTab::Groups {
            if let Some(g) = group {
                let linked: Vec<&TeamRole> = self.roles.iter().filter(|r| r.groups.iter().any(|x| x.id == g.id)).collect();
                let mut actions = div();
                if can_groups {
                    let mut colors = div().flex().flex_wrap().gap(px(6.));
                    for c in ROLE_COLORS {
                        let id = g.id.clone();
                        colors = colors.child(color_dot(SharedString::from(format!("gc-{c}")), c, g.color.as_deref() == Some(c), p).on_click(cx.listener(move |this, _, window, cx| {
                            let id = id.clone();
                            let label = "Grup güncellendi.".to_string();
                            this.team_action(label.clone(), label, move |api, pid| api.update_project_user_group(pid, &id, serde_json::json!({ "color": c })).map(|_| serde_json::Value::Null), window, cx);
                        })));
                    }
                    let del = g.clone();
                    actions = actions.child(colors).child(settings_button("group-delete", "Grubu sil", Btn::Danger, self.busy, p).h(px(28.)).on_click(cx.listener(move |this, _, window, cx| {
                        let (id, label) = (del.id.clone(), format!("{} grubu silindi.", del.name));
                        this.team_action(label.clone(), label, move |api, pid| api.delete_project_user_group(pid, &id), window, cx);
                    })));
                }
                let mut lines = vec![format!("@{} · {} kişi · {} bağlı rol", g.handle, g.members.len(), linked.len())];
                if let Some(d) = g.description.clone().filter(|d| !d.is_empty()) {
                    lines.push(d);
                }
                detail = detail.child(Self::team_head(div().size(px(20.)).flex_none().rounded_full().bg(hex(g.color.as_deref(), p.neutral_fallback)).into_any_element(), text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(g.name.clone()).into_any_element(), lines, actions, p));
                let mut row = chips();
                for person in &g.members {
                    let (gid, uid) = (g.id.clone(), person.id.clone());
                    let mut t = tag(&person.username, None, p);
                    if can_groups {
                        t = t.child(tag_remove(SharedString::from(format!("gx-{}", person.id)), p).on_click(cx.listener(move |this, _, window, cx| this.group_member(gid.clone(), uid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if g.members.is_empty() {
                    row = row.child(small("Bu grupta henüz kimse yok."));
                }
                if can_groups {
                    let options: Vec<(String, String)> = self.members.iter().filter(|m| !g.members.iter().any(|x| x.id == m.id)).map(|m| (m.id.clone(), m.display_name.clone())).collect();
                    row = row.child(self.picker_view(Picker::AddToGroup(g.id.clone()), "＋ kişi ekle", "Herkes eklendi", options, p, cx));
                }
                detail = detail.child(block("Üyeler").child(row));
                let mut row = chips();
                for r in &linked {
                    let (rid, gid) = (r.id.clone(), g.id.clone());
                    let mut t = tag(&r.handle, r.color.as_deref(), p);
                    if can_roles {
                        t = t.child(tag_remove(SharedString::from(format!("gl-{}", r.id)), p).on_click(cx.listener(move |this, _, window, cx| this.link_group(rid.clone(), gid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if linked.is_empty() {
                    row = row.child(small("Bu gruba bağlı rol yok — grubun yetkisi yalnızca @everyone rolünden gelir."));
                }
                if can_roles {
                    let options: Vec<(String, String)> = self.roles.iter().filter(|r| !r.everyone && !r.founder && !linked.iter().any(|x| x.id == r.id)).map(|r| (r.id.clone(), r.name.clone())).collect();
                    let empty = if self.roles.iter().any(|r| !r.everyone && !r.founder) { "Tüm roller bağlı" } else { "Önce Roller sekmesinden bir rol oluştur" };
                    row = row.child(self.picker_view(Picker::LinkRole(g.id.clone()), "＋ rol bağla", empty, options, p, cx));
                }
                detail = detail.child(block("Bağlı roller").child(hint("Bu gruba bağlanan rolün yetkileri, gruptaki herkese geçer.", p)).child(row));
            }
        } else if let Some(r) = role {
            let mut actions = div();
            if can_roles && !r.founder && !r.everyone {
                let mut colors = div().flex().flex_wrap().gap(px(6.));
                for c in ROLE_COLORS {
                    let id = r.id.clone();
                    colors = colors.child(color_dot(SharedString::from(format!("rc-{c}")), c, r.color.as_deref() == Some(c), p).on_click(cx.listener(move |this, _, window, cx| {
                        let id = id.clone();
                        let label = "Rol güncellendi.".to_string();
                        this.team_action(label.clone(), label, move |api, pid| api.update_project_role(pid, &id, serde_json::json!({ "color": c })), window, cx);
                    })));
                }
                let del = r.clone();
                actions = actions.child(colors).child(settings_button("role-delete", "Rolü sil", Btn::Danger, self.busy, p).h(px(28.)).on_click(cx.listener(move |this, _, window, cx| {
                    let (id, label) = (del.id.clone(), format!("{} silindi.", del.name));
                    this.team_action(label.clone(), label, move |api, pid| api.delete_project_role(pid, &id), window, cx);
                })));
            }
            if r.founder || r.everyone {
                actions = actions.child(
                    div().px(px(9.)).py(px(3.)).border_1().border_color(css_rgba(255, 181, 69, 0.5)).rounded(radius::PILL).bg(p.warning_soft).text_color(p.warning_text).text_size(fs(9.5)).line_height(lh(fs(9.5))).font_weight(FontWeight::BOLD).child("temel rol"),
                );
            }
            let mut lines = vec![format!(
                "@{} · {}",
                r.handle,
                if r.founder { "temel rol · proje sahipleri".to_string() } else if r.everyone { "temel rol · projedeki herkes".into() } else { format!("{} kişi · {} grup", r.members.len(), r.groups.len()) }
            )];
            if let Some(d) = r.description.clone().filter(|d| !d.is_empty()) {
                lines.push(d);
            }
            detail = detail.child(Self::team_head(div().size(px(20.)).flex_none().rounded_full().bg(hex(r.color.as_deref(), p.neutral_fallback)).into_any_element(), text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(r.name.clone()).into_any_element(), lines, actions, p));
            if !r.founder && !r.everyone {
                let mut row = chips();
                for person in &r.members {
                    let (uid, rid) = (person.id.clone(), r.id.clone());
                    let mut t = tag(&person.username, None, p);
                    if can_roles {
                        t = t.child(tag_remove(SharedString::from(format!("px-{}", person.id)), p).on_click(cx.listener(move |this, _, window, cx| this.assign(uid.clone(), rid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if r.members.is_empty() {
                    row = row.child(small("Bu rol henüz doğrudan kimseye verilmedi."));
                }
                if can_roles {
                    let options: Vec<(String, String)> = self.members.iter().filter(|m| !r.members.iter().any(|x| x.id == m.id)).map(|m| (m.id.clone(), m.display_name.clone())).collect();
                    row = row.child(self.picker_view(Picker::GivePerson(r.id.clone()), "＋ kişiye ver", "Herkeste var", options, p, cx));
                }
                detail = detail.child(block("Bu role sahip kişiler").child(row));
                let mut row = chips();
                for g in &r.groups {
                    let (rid, gid) = (r.id.clone(), g.id.clone());
                    let mut t = tag(&g.handle, g.color.as_deref(), p);
                    if can_roles {
                        t = t.child(tag_remove(SharedString::from(format!("rg-{}", g.id)), p).on_click(cx.listener(move |this, _, window, cx| this.link_group(rid.clone(), gid.clone(), false, window, cx))));
                    }
                    row = row.child(t);
                }
                if r.groups.is_empty() {
                    row = row.child(small("Bu role bağlı grup yok."));
                }
                if can_roles {
                    let options: Vec<(String, String)> = self.groups.iter().filter(|g| !r.groups.iter().any(|x| x.id == g.id)).map(|g| (g.id.clone(), g.name.clone())).collect();
                    let empty = if self.groups.is_empty() { "Önce Gruplar sekmesinden bir grup oluştur" } else { "Tüm gruplar bağlı" };
                    row = row.child(self.picker_view(Picker::LinkGroup(r.id.clone()), "＋ grup bağla", empty, options, p, cx));
                }
                detail = detail.child(block("Bağlı gruplar").child(row));
            }
            let hint_text = if r.founder { "Kurucu rolünde tüm yetkiler her zaman açıktır ve değiştirilemez." } else if r.everyone { "Buradaki her yetki projedeki herkeste varsayılan olarak açılır." } else { "Açtığın yetkiler bu projede bu role sahip herkese verilir." };
            let mut perms = block("Yetkiler").child(hint(hint_text, p));
            let mut groups: Vec<&str> = Vec::new();
            for item in &self.catalog {
                if !groups.contains(&item.group.as_str()) {
                    groups.push(&item.group);
                }
            }
            for g in groups {
                let mut items = Vec::new();
                for item in self.catalog.iter().filter(|i| i.group == g) {
                    let on = r.permissions.contains(&item.id);
                    let disabled = self.busy || !can_roles || r.founder;
                    let (role, pid) = (r.clone(), item.id.clone());
                    items.push(
                        perm_toggle(SharedString::from(format!("perm-{}", item.id)), &item.label, &item.hint, on, disabled, p)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !disabled {
                                    this.toggle_permission(&role, &pid, window, cx);
                                }
                            }))
                            .into_any_element(),
                    );
                }
                perms = perms.child(div().mt(px(8.)).flex().flex_col().gap(px(4.)).child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(g))).child(perm_grid(items, detail_w, 320.)));
            }
            detail = detail.child(perms);
        }

        let explainer: Div = match self.tab {
            TeamTab::Groups => div().child("Gruplar @etiketle bahsedilen ekiplerdir (ör. @tasarım). Gruba rol bağlarsan gruptaki herkes o rolün yetkilerini alır."),
            _ => div()
                .flex()
                .flex_wrap()
                .child("Basit akış:\u{a0}")
                .child(div().font_weight(FontWeight::BOLD).child("Roller"))
                .child("\u{a0}bölümünde bir rol oluşturup yetkilerini aç; sonra\u{a0}")
                .child(div().font_weight(FontWeight::BOLD).child("Üyeler"))
                .child("\u{a0}bölümünden bu rolü kişiye ver. Roller yalnız bu projede geçerlidir."),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(tabs)
            .child(explainer.text_size(fs(10.)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3))
            .child(div().flex().items_start().gap(px(14.)).child(list).child(detail.p2_enter(("team-detail", self.tab as usize), Entrance::Rise, std::time::Duration::ZERO)))
            .into_any_element()
    }

    /// AdminAccess.tsx: kişi seç → görebileceği projeler ve kanallar.
    fn access_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = &self.access else {
            return match &self.access_error {
                Some(e) => warn(e.clone(), p).into_any_element(),
                None => hint("Erişim listesi yükleniyor…", p).into_any_element(),
            };
        };
        let selected = data.members.iter().find(|m| Some(&m.id) == self.access_user.as_ref()).cloned();
        let busy = self.busy;
        let mut people = div().flex().flex_col().gap(px(5.));
        for m in &data.members {
            let id = m.id.clone();
            let active = selected.as_ref().is_some_and(|s| s.id == m.id);
            let hover = p.bg_hover;
            people = people.child(
                div()
                    .id(SharedString::from(format!("acc-{}", m.id)))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(7.))
                    .border_1()
                    .rounded(radius::MD)
                    .cursor_pointer()
                    .map(|s| if active { s.border_color(p.border_accent).bg(p.accent_soft) } else { s.border_color(gpui_kit::transparent_black()).hover(move |s| s.bg(hover)) })
                    .child(div().w(px(28.)).flex_none().child(admin_avatar(&m.id, &m.display_name, true, p)))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(m.display_name.clone())).child(text(fs(9.)).text_color(p.text_3).child(format!("@{}{}", m.username, if m.owner { " · Proje sahibi" } else { "" }))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.access_user = Some(id.clone());
                        cx.notify();
                    })),
            );
        }
        let mut projects = div().id("access-projects").flex_1().min_h_0().overflow_y_scroll().pr(px(4.)).flex().flex_col().gap(px(5.));
        if let Some(sel) = &selected {
            for pr in &data.projects {
                let visible = sel.owner || pr.owner_id == sel.id || pr.member_ids.contains(&sel.id);
                let protected = sel.owner || pr.owner_id == sel.id;
                let shown = pr.channels.iter().filter(|c| sel.owner || c.visible_user_ids.contains(&sel.id)).count();
                let (pid, uid) = (pr.id.clone(), sel.id.clone());
                let switch_off = protected || busy;
                // `.admin-access-switch`: 28×16 anahtar + etiket.
                let switch = div()
                    .id(SharedString::from(format!("accp-{}", pr.id)))
                    .ml_auto()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .when(!switch_off, |s| s.cursor_pointer())
                    .child(
                        div()
                            .relative()
                            .w(px(28.))
                            .h(px(16.))
                            .rounded_full()
                            .when(switch_off, |s| s.opacity(0.55))
                            .map(|s| if visible { s.bg(p.accent_grad.background()) } else { s.bg(p.navy_500) })
                            .child(div().absolute().top(px(3.)).left(px(if visible { 15. } else { 3. })).size(px(10.)).rounded_full().bg(gpui_kit::white())),
                    )
                    .child(text(fs(9.5)).css_weight(650.).text_color(p.text_2).whitespace_nowrap().when(switch_off, |s| s.opacity(0.55)).child(if visible { "Projenin üyesi" } else { "Üye değil" }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !switch_off {
                            this.set_access(true, pid.clone(), uid.clone(), !visible, window, cx);
                        }
                    }));
                let article = div()
                    .flex_none()
                    .w_full()
                    .overflow_hidden()
                    .border_1()
                    .border_color(if visible { p.border_strong } else { p.border })
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .when(!visible, |s| s.opacity(0.72))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_x(px(10.))
                            .gap_y(px(8.))
                            .px(px(11.))
                            .py(px(10.))
                            .bg(p.bg_2)
                            .child(div().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(pr.name.clone())).child(text(fs(9.)).text_color(p.text_3).child(format!("{} kanal · {}", pr.channels.len(), if visible { format!("{shown} görünür") } else { "erişim kapalı".into() }))))
                            .child(switch),
                    );
                let mut channels = div().flex().flex_col().gap(px(7.)).p(px(7.)).overflow_hidden();
                let mut categories: Vec<&str> = Vec::new();
                for c in &pr.channels {
                    if !categories.contains(&c.category.as_str()) {
                        categories.push(&c.category);
                    }
                }
                for cat in categories {
                    let rows: Vec<AnyElement> = pr
                        .channels
                        .iter()
                        .filter(|c| c.category == cat)
                        .map(|c| {
                            let on = visible && (sel.owner || c.visible_user_ids.contains(&sel.id));
                            let locked = !visible || sel.owner || busy;
                            let (cid, uid) = (c.id.clone(), sel.id.clone());
                            let hover = p.bg_hover;
                            div()
                                .id(SharedString::from(format!("accc-{}", c.id)))
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(7.))
                                .p(px(7.))
                                .rounded(radius::XS)
                                .when(!visible, |s| s.opacity(0.38))
                                .when(visible, |s| s.cursor_pointer().hover(move |s| s.bg(hover)))
                                .child(div().w(px(20.)).flex_none().flex().justify_center().child(channel_icon(&c.kind, 15., p.text_accent)))
                                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(c.name.clone())).child(text(fs(9.)).text_color(p.text_3).child(type_label(&c.kind))))
                                .child(div().when(locked, |s| s.opacity(0.6)).child(native_checkbox(on, p)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if !locked {
                                        this.set_access(false, cid.clone(), uid.clone(), !on, window, cx);
                                    }
                                }))
                                .into_any_element()
                        })
                        .collect();
                    let mut grid = div().flex().flex_col().gap(px(1.));
                    let mut it = rows.into_iter();
                    loop {
                        let Some(a) = it.next() else { break };
                        let b = it.next().unwrap_or_else(|| div().flex_1().into_any_element());
                        grid = grid.child(div().flex().gap(px(1.)).child(a).child(b));
                    }
                    channels = channels.child(div().flex().flex_col().gap(px(3.)).child(div().pt(px(3.)).px(px(7.)).pb(px(1.)).text_size(fs(8.)).line_height(lh(fs(8.))).font_weight(FontWeight::BOLD).tracking(0.12).text_color(p.text_accent).child(tr_upper(cat))).child(grid));
                }
                if pr.channels.is_empty() {
                    channels = channels.child(hint("Bu projede kanal yok.", p));
                }
                projects = projects.child(article.child(channels));
            }
            projects = projects.child(self.overrides_view(sel, p, cx));
        }
        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .when_some(self.access_error.clone(), |s, e| s.child(warn(e, p)))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(10.))
                    .child(div().w(px(220.)).flex_none().min_h_0().overflow_hidden().flex().flex_col().gap(px(9.)).child(block_title("Kişi seç", p)).child(hint("Bir kişinin bu projede görebileceği kanalları ve rollerinden bağımsız özel yetkilerini belirle.", p)).child(people))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .gap(px(9.))
                            .child(block_title(format!("{} için görünür içerikler", selected.as_ref().map(|s| s.display_name.clone()).unwrap_or_else(|| "Kişi".into())), p))
                            .when(selected.as_ref().is_some_and(|s| s.owner), |s| s.child(hint("Proje sahibi bu projedeki bütün kanalları görür ve bütün yetkilere sahiptir.", p)))
                            .child(projects),
                    ),
            )
            .into_any_element()
    }

    /// İzinler › kişiye özel yetkiler (`ProjectMember.permissions` / `deniedPermissions`): her yetki için
    /// Engelle / Varsayılan / İzin ver (PermissionRow.tsx `.perm-switch`).
    fn overrides_view(&self, sel: &p2_client_core::AccessMember, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div().flex_none().mt(px(6.)).flex().flex_col().gap(px(6.)).child(block_title("Kişiye özel yetkiler", p));
        if sel.owner {
            return col.child(hint("Proje sahibinin yetkileri değiştirilemez.", p)).into_any_element();
        }
        if self.catalog.is_empty() {
            return col.child(hint("Yetki kataloğu yüklenemedi.", p)).into_any_element();
        }
        let o = self.overrides.get(&sel.id).cloned().unwrap_or_default();
        col = col.child(hint("Varsayılan, kişinin rollerinden gelen yetkiyi kullanır. İzin ver / Engelle yalnız bu kişi için, yalnız bu projede geçerlidir.", p));
        let mut groups: Vec<&str> = Vec::new();
        for item in &self.catalog {
            if !groups.contains(&item.group.as_str()) {
                groups.push(&item.group);
            }
        }
        for g in groups {
            let mut rows = div().flex().flex_col().gap(px(4.));
            for item in self.catalog.iter().filter(|i| i.group == g) {
                let state = if o.allow.contains(&item.id) { "allow" } else if o.deny.contains(&item.id) { "deny" } else { "inherit" };
                let effective = o.effective.contains(&item.id);
                let sub = format!("{} · şu an {}", item.hint, if effective { "açık" } else { "kapalı" });
                rows = rows.child(perm_line(SharedString::from(format!("ov-{}", item.id)), &item.label, &sub, state, false, p).child(self.override_switch(&sel.id, &item.id, state, p, cx)));
            }
            col = col.child(div().mt(px(4.)).flex().flex_col().gap(px(4.)).child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(g))).child(rows));
        }
        col.into_any_element()
    }

    /// PermissionRow.tsx `.perm-switch`: p 2, kenarlık, r 8, bg-1; 30×24 düğmeler (✕ ／ ✓).
    fn override_switch(&self, user: &str, perm: &str, state: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let disabled = self.busy;
        let mut sw = div().flex().flex_none().p(px(2.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_1);
        for (value, glyph, title) in [("deny", "✕", "Engelle"), ("inherit", "／", "Varsayılan"), ("allow", "✓", "İzin ver")] {
            let on = state == value;
            let (bg, fg) = match (on, value) {
                (true, "allow") => (Some(p.success_soft), p.success_text),
                (true, "deny") => (Some(p.danger_soft), p.danger_text),
                (true, _) => (Some(p.bg_hover), p.text_strong),
                _ => (None, p.text_3),
            };
            let (hover, strong) = (p.bg_hover, p.text_strong);
            let (user, perm) = (user.to_string(), perm.to_string());
            sw = sw.child(
                div()
                    .id(SharedString::from(format!("ovs-{perm}-{value}")))
                    .w(px(30.))
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::XS)
                    .text_size(fs(11.))
                    .line_height(px(11.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .text_color(fg)
                    .when_some(bg, |s, b| s.bg(b))
                    .when(!disabled, |s| s.cursor_pointer().hover(move |s| s.bg(hover).text_color(strong)))
                    .tooltip(move |window, cx| p2_ui::controls::tooltip(title, window, cx))
                    .child(glyph)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !on && !disabled {
                            this.set_override(user.clone(), perm.clone(), value, window, cx);
                        }
                    })),
            );
        }
        sw.into_any_element()
    }

    fn invites_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can = self.perms.can("member.invite");
        let server = self.api.base().trim_end_matches('/').trim_end_matches("/api").to_string();
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, inv) in self.invites.iter().enumerate() {
            let (code, revoke) = (inv.code.clone(), inv.clone());
            let revoke_off = self.busy || !can;
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(11.))
                    .px(px(10.))
                    .py(px(9.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .child(div().flex_none().px(px(11.)).py(px(7.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_1).font_family(FONT_MONO).text_size(fs(12.)).line_height(lh(fs(12.))).font_weight(FontWeight::BOLD).tracking(0.08).text_color(p.text_strong).child(inv.code.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(fs(10.)).text_color(p.text_3).child(match inv.max_uses {
                                Some(max) => format!("{}/{max} kullanım", inv.use_count),
                                None => format!("{} kullanım · sınırsız", inv.use_count),
                            }))
                            .child(text(fs(10.)).text_color(p.text_3).child(format!(
                                "{} · {} oluşturdu",
                                inv.expires_at.as_deref().map(|e| format!("{} tarihinde biter", day(e))).unwrap_or_else(|| "Süresiz".into()),
                                inv.created_by.display_name
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(settings_button(SharedString::from(format!("inv-copy-{i}")), "Kopyala", Btn::Plain, false, p).h(px(30.)).on_click(cx.listener(move |this, _, _, cx| this.copy_invite(&code, cx))))
                            .child(settings_button(SharedString::from(format!("inv-rev-{i}")), "İptal et", Btn::Danger, revoke_off, p).h(px(30.)).on_click(cx.listener(move |this, _, window, cx| {
                                if !revoke_off {
                                    this.revoke_invite(revoke.clone(), window, cx);
                                }
                            }))),
                    )
                    .p2_enter(SharedString::from(format!("inv-{}", inv.id)), Entrance::Rise, std::time::Duration::ZERO),
            );
        }
        if self.invites.is_empty() {
            rows = rows.child(hint("Etkin davet kodu yok. Yeni bir kod oluştur ya da varsayılan kodu yenile.", p));
        }
        const SERVER_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/></svg>"#;
        let copy_server = server.clone();
        div()
            .flex()
            .flex_col()
            .child(
                settings_block("Sunucu bağlantısı", false, p).child(hint("Yeni kullanıcı önce bu sunucu adresiyle P2'ye bağlanır, ardından aşağıdaki kodlardan birini girer. Bir kod yalnız bu projeye sokar; kişi başka projeleri görmez.", p)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(12.))
                        .py(px(10.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(radius::MD)
                        .bg(p.bg_1)
                        .child(div().size(px(38.)).flex_none().flex().items_center().justify_center().rounded(radius::SM).bg(p.accent_soft).child(gpui_kit::svg().data(SERVER_SVG).size(px(18.)).text_color(p.text_accent)))
                        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(10.)).font_weight(FontWeight::BOLD).tracking(0.06).text_color(p.text_3).child("SUNUCU ADRESİ")).child(text(fs(13.)).font_family(FONT_MONO).font_weight(FontWeight::BOLD).text_color(p.text_strong).truncate().child(server)))
                        .child(settings_button("inv-server-copy", "Kopyala", Btn::Primary, false, p).h(px(34.)).on_click(cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(copy_server.clone()));
                            crate::toasts::ok("Sunucu bağlantısı kopyalandı.");
                            cx.notify();
                        }))),
                ),
            )
            .child(
                settings_block("Yeni davet kodu", false, p).child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(9.))
                        .child(div().flex_1().min_w_0().child(settings_field("Kullanım sınırı", field_input(&self.invite_uses, false, p, window, cx), p)))
                        .child(div().flex_1().min_w_0().child(settings_field("Geçerlilik (gün)", field_input(&self.invite_days, false, p, window, cx), p)))
                        .child(settings_button("inv-create", "Kod oluştur", Btn::Primary, self.busy || !can, p).h(px(34.)).on_click(cx.listener(move |this, _, window, cx| {
                            if can {
                                this.create_invite(window, cx)
                            }
                        }))),
                ),
            )
            .child(
                settings_block(format!("Davet kodları · {}", self.invites.len()), true, p)
                    .child(div().flex().flex_wrap().gap(px(8.)).child(settings_button("inv-rotate", "Varsayılan kodu yenile", Btn::Plain, self.busy || !can, p).on_click(cx.listener(move |this, _, window, cx| {
                        if can {
                            this.rotate_invite(window, cx)
                        }
                    }))))
                    .child(rows),
            )
            .into_any_element()
    }

    /// Proje ayarları (`PATCH /projects/:p/admin`): ad, açıklama, ikon, kapak, başlangıç kanalı,
    /// kütüphane. Taslakta değişen alanlar "Kaydet" ile tek istekte gider.
    fn settings_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(project) = self.project().cloned() else { return hint("Proje yok.", p).into_any_element() };
        let pid = project.id.clone();
        let can_library = self.perms.can("release.publish");
        let (busy_icon, busy_cover) = (self.uploading == Some(false), self.uploading == Some(true));
        let dirty = !self.settings_patch(cx).is_empty();
        let name_now = self.project_name.read(cx).value().trim().to_string();
        let save_off = self.busy || !dirty || name_now.chars().count() < 2;
        let image = |key: String, url: &Option<String>, r: f32| -> Option<AnyElement> {
            url.as_ref()?;
            crate::avatars::remember(&key, url.as_deref());
            crate::avatars::image(&key).map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(r)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element())
        };
        // Kimlik: ikon kutusu (72px, proje rayıyla aynı ton gradyanı) + düğmeler.
        let icon_key = format!("project-icon:{pid}:{}", self.draft_icon.clone().unwrap_or_default());
        let face = image(icon_key, &self.draft_icon, 20.).unwrap_or_else(|| p2_ui::components::initials_label(p2_ui::components::user_initials(&project.name)).into_any_element());
        let hue = p.hues[p2_ui::components::avatar_hue(&pid) % 10];
        let icon_box = div()
            .id("as-icon")
            .size(px(72.))
            .flex_none()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(20.))
            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
            .text_color(hue.fg)
            .text_size(fs(22.))
            .line_height(px(72.))
            .font_weight(FontWeight::EXTRA_BOLD)
            .shadow(shadows(&p.shadow_md))
            .cursor_pointer()
            .child(face)
            .on_click(cx.listener(move |this, _, window, cx| {
                if !busy_icon {
                    this.pick_image(false, window, cx)
                }
            }));
        let icon_row = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(icon_box)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(settings_button("as-icon-pick", if busy_icon { "Yükleniyor…" } else if self.draft_icon.is_some() { "İkonu değiştir" } else { "İkon yükle" }, Btn::Plain, busy_icon, p).on_click(cx.listener(move |this, _, window, cx| {
                        if !busy_icon {
                            this.pick_image(false, window, cx)
                        }
                    })))
                    .when(self.draft_icon.is_some(), |s| {
                        s.child(settings_button("as-icon-x", "Kaldır", Btn::Plain, false, p).on_click(cx.listener(|this, _, _, cx| {
                            this.draft_icon = None;
                            cx.notify();
                        })))
                    })
                    .child(div().w_full().child(hint("Kare görsel; proje rayında, davet önizlemesinde ve kütüphanede görünür.", p))),
            );
        // Kapak: 16:9 şerit (yüksekliği 150px ile sınırlı).
        let cover_key = format!("project-cover:{pid}:{}", self.draft_cover.clone().unwrap_or_default());
        let (soft, accent) = (p.accent_soft, p.border_accent);
        let cover = div()
            .id("as-cover")
            .w_full()
            .h(px(150.))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_dashed()
            .border_color(p.border_strong)
            .rounded(radius::LG)
            .bg(p.bg_1)
            .text_color(p.text_3)
            .text_size(fs(11.))
            .cursor_pointer()
            .hover(move |s| s.bg(soft).border_color(accent))
            .child(image(cover_key, &self.draft_cover, 14.).unwrap_or_else(|| (if busy_cover { "Yükleniyor…" } else { "Kapak seç (16:9 önerilir)" }).into_any_element()))
            .on_click(cx.listener(move |this, _, window, cx| {
                if !busy_cover {
                    this.pick_image(true, window, cx)
                }
            }));
        let cover_actions = div()
            .flex()
            .gap(px(8.))
            .child(settings_button("as-cover-pick", if self.draft_cover.is_some() { "Kapağı değiştir" } else { "Kapak yükle" }, Btn::Plain, busy_cover, p).on_click(cx.listener(move |this, _, window, cx| {
                if !busy_cover {
                    this.pick_image(true, window, cx)
                }
            })))
            .when(self.draft_cover.is_some(), |s| {
                s.child(settings_button("as-cover-x", "Kaldır", Btn::Plain, false, p).on_click(cx.listener(|this, _, _, cx| {
                    this.draft_cover = None;
                    cx.notify();
                })))
            });
        // Başlangıç kanalı: ses dışı kanallar (silinenler hariç).
        let options: Vec<(String, String)> = std::iter::once((String::new(), "Yok (ilk kanal açılır)".to_string()))
            .chain(self.channels.iter().filter(|c| c.deleted_at.is_none() && c.kind != "VOICE").map(|c| (c.id.clone(), format!("#{} · {}", c.name, c.category))))
            .collect();
        let current = self.draft_default.as_ref().and_then(|id| self.channels.iter().find(|c| &c.id == id)).map(|c| format!("#{}", c.name)).unwrap_or_else(|| "Başlangıç kanalı yok".into());
        let default_row = div().flex().items_center().gap(px(10.)).child(div().flex_1().min_w_0().h(px(34.)).flex().items_center().px(px(10.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_3).text_size(fs(12.)).text_color(p.text).truncate().child(current)).child(self.picker_view(Picker::DefaultChannel, "Değiştir", "Kanal yok", options, p, cx));
        let library = self.draft_library;
        // Keşfet: görünürlük karoları, (açıksa) kategori çipleri ve tanıtım satırı.
        let discover_block = {
            use crate::screens::discover_ui as dui;
            let public = self.draft_visibility == p2_client_core::VISIBILITY_PUBLIC;
            let tiles = div()
                .flex()
                .gap(px(10.))
                .child(dui::choice_tile("as-vis-private", dui::glyph::LOCK, p2_i18n::t("vis.private"), p2_i18n::t("vis.private.hint"), !public, p).on_click(cx.listener(|this, _, _, cx| {
                    this.draft_visibility = p2_client_core::VISIBILITY_PRIVATE.to_string();
                    cx.notify();
                })))
                .child(dui::choice_tile("as-vis-public", dui::glyph::GLOBE, p2_i18n::t("vis.public"), p2_i18n::t("vis.public.hint"), public, p).on_click(cx.listener(|this, _, _, cx| {
                    this.draft_visibility = p2_client_core::VISIBILITY_PUBLIC.to_string();
                    cx.notify();
                })));
            let mut chips = div().flex().flex_wrap().gap(px(7.));
            for id in p2_client_core::DISCOVER_CATEGORIES {
                let on = self.draft_category.as_deref() == Some(*id);
                chips = chips.child(dui::category_chip(gpui_kit::SharedString::from(format!("as-cat-{id}")), dui::category_glyph(id), dui::category_label(id), None, on, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.draft_category = if this.draft_category.as_deref() == Some(*id) { None } else { Some(id.to_string()) };
                    cx.notify();
                })));
            }
            settings_block(p2_i18n::t("vis.block"), false, p)
                .child(hint(p2_i18n::t("vis.block.hint"), p))
                .child(settings_field(&p2_i18n::t("vis.label"), tiles, p))
                .child(settings_field(&p2_i18n::t("vis.category"), chips, p))
                .child(settings_field(&p2_i18n::t("vis.tagline"), field_input(&self.project_tagline, false, p, window, cx), p))
        };
        div()
            .flex()
            .flex_col()
            .child(
                settings_block("Kimlik", false, p)
                    .child(settings_field("Proje adı", field_input(&self.project_name, false, p, window, cx), p))
                    .child(settings_field("Açıklama", field_input(&self.project_desc, false, p, window, cx), p))
                    .child(hint("Ad ve açıklama davet önizlemesinde de görünür.", p))
                    .child(settings_field("İkon", icon_row, p)),
            )
            .child(settings_block("Kapak", false, p).child(cover).child(cover_actions))
            .child(settings_block("Başlangıç kanalı", false, p).child(hint("Projeye girenlerin ilk açılan kanalı.", p)).child(default_row))
            .child(discover_block)
            .child(
                settings_block("Kütüphane", false, p).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).text_color(p.text).child("Kütüphanede uygulama olarak listele")).child(text(fs(10.)).text_color(p.text_3).child(if can_library { "Açıkken proje Kütüphane'de oyun/uygulama olarak görünür; sürüm yüklenip çalıştırılabilir." } else { "Değiştirmek için bu projede “Sürüm yayınla” yetkisi gerekir." })))
                        .child(div().when(!can_library, |s| s.opacity(0.5)).child(p2_ui::form::toggle("as-library", library, 0, p).on_click(cx.listener(move |this, _, _, cx| {
                            if can_library {
                                this.draft_library = !library;
                                cx.notify();
                            }
                        })))),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(settings_button("as-save", if self.busy { "Kaydediliyor…" } else { "Kaydet" }, Btn::Primary, save_off, p).on_click(cx.listener(move |this, _, window, cx| {
                        if !save_off {
                            this.save_settings(window, cx)
                        }
                    })))
                    .when(dirty, |s| {
                        s.child(settings_button("as-reset", "Değişiklikleri geri al", Btn::Plain, self.busy, p).on_click(cx.listener(|this, _, window, cx| {
                            if let Some(pr) = this.project().cloned() {
                                this.project_name.update(cx, |s, cx| s.set_value(pr.name.clone(), window, cx));
                                this.project_desc.update(cx, |s, cx| s.set_value(pr.description.clone().unwrap_or_default(), window, cx));
                                (this.draft_icon, this.draft_cover, this.draft_default, this.draft_library) = (pr.icon_url, pr.cover_url, pr.default_channel_id, pr.library_enabled);
                            }
                            this.reset_discover_draft(window, cx);
                            cx.notify();
                        })))
                    })
                    .when(dirty, |s| s.child(hint("Kaydedilmemiş değişiklikler var.", p))),
            )
            .into_any_element()
    }

    /// Yasaklılar (`GET /projects/:p/admin/bans`): kim, kim yasakladı, ne zaman, neden; yasağı kaldır.
    fn bans_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let can = self.perms.can("member.kick");
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, b) in self.bans.iter().enumerate() {
            let it = b.clone();
            let busy = self.busy || !can;
            let by = b.banned_by.as_ref().map(|u| format!(" · {} yasakladı", u.display_name)).unwrap_or_default();
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(11.))
                    .px(px(10.))
                    .py(px(9.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .child(div().relative().flex_none().child(admin_avatar(&b.user.id, &b.user.display_name, true, p)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(b.user.display_name.clone()))
                            .child(text(fs(10.)).text_color(p.text_3).truncate().child(format!("@{} · {}{by}", b.user.username, day(&b.created_at))))
                            .child(text(fs(10.)).text_color(if b.reason.as_deref().is_some_and(|r| !r.is_empty()) { p.text_2 } else { p.text_4 }).truncate().child(b.reason.clone().filter(|r| !r.is_empty()).map(|r| format!("Neden: {r}")).unwrap_or_else(|| "Neden belirtilmedi".into()))),
                    )
                    .child(settings_button(SharedString::from(format!("unban-{i}")), "Yasağı kaldır", Btn::Plain, busy, p).h(px(30.)).on_click(cx.listener(move |this, _, window, cx| {
                        if !busy {
                            this.unban(it.clone(), window, cx)
                        }
                    })))
                    .p2_enter(SharedString::from(format!("ban-{}", b.user_id)), Entrance::Rise, std::time::Duration::ZERO),
            );
        }
        if self.bans.is_empty() {
            rows = rows.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .py(px(28.))
                    .border_1()
                    .border_dashed()
                    .border_color(p.border)
                    .rounded(radius::MD)
                    .child(text(fs(13.)).font_weight(FontWeight::BOLD).text_color(p.text_2).child("Yasaklı kimse yok"))
                    .child(hint("Üyeler bölümünden bir kişiyi yasakladığında burada görünür.", p)),
            );
        }
        settings_block(format!("Yasaklılar · {}", self.bans.len()), true, p)
            .child(hint("Yasaklanan kişi projeden çıkarılır ve yasak kalkana kadar hiçbir davet koduyla geri katılamaz. Yasağı kaldırmak onu projeye geri eklemez; yeni bir davet koduyla katılabilir.", p))
            .child(rows)
            .into_any_element()
    }

    /// Sunucu yönetimi › Oturumlar: kişinin bütün oturumlarını kapat (yalnız kurulum sahibi).
    fn sessions_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for (i, m) in self.people.iter().enumerate() {
            let it = m.clone();
            let busy = self.busy || m.id == self.me;
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(11.))
                    .px(px(10.))
                    .py(px(8.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .child(div().relative().flex_none().child(admin_avatar(&m.id, &m.display_name, true, p)).child(presence(&m.status, p)))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(m.display_name.clone())).child(text(fs(10.)).text_color(p.text_3).truncate().child(format!("@{} · {}", m.username, status_label(&m.status)))))
                    .child(settings_button(SharedString::from(format!("ses-{i}")), "Oturumları kapat", Btn::Danger, busy, p).h(px(30.)).on_click(cx.listener(move |this, _, _, cx| {
                        if !busy {
                            this.confirm = Some(Confirm::Sessions(it.clone()));
                            cx.notify();
                        }
                    }))),
            );
        }
        if self.people.is_empty() {
            rows = rows.child(hint("Benimle proje paylaşan kimse yok.", p));
        }
        settings_block(format!("Kişiler · {}", self.people.len()), true, p)
            .child(hint("Bir hesabın ele geçirildiğinden şüpheleniyorsan bütün cihazlardaki oturumlarını kapatabilirsin. Proje yöneticileri bunu yapamaz; yalnız kurulum sahibi.", p))
            .child(rows)
            .into_any_element()
    }

    /// `.admin-channel`: ızgara `26px 1fr auto`, p 9 10, kenarlık, r 11, bg-3.
    fn channel_row(id: String, icon_el: AnyElement, name: String, subs: Vec<String>, trail: Option<u64>, deleted: bool, p: &Palette) -> Stateful<Div> {
        div()
            .flex()
            .items_center()
            .gap(px(11.))
            .px(px(10.))
            .py(px(9.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.bg_3)
            .when(deleted, |s| s.opacity(0.6))
            .child(div().w(px(26.)).flex_none().flex().justify_center().text_color(p.text_accent).child(icon_el))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(name)).children(subs.into_iter().map(|s| text(fs(10.)).text_color(p.text_3).truncate().child(s))))
            .when_some(trail, |s, n| s.child(div().flex().flex_col().items_end().text_color(p.text_strong).child(text(fs(14.)).font_weight(FontWeight::BOLD).child(n.to_string())).child(text(fs(9.)).css_weight(600.).text_color(p.text_3).child("mesaj"))))
            .id(SharedString::from(format!("ach-{id}")))
    }

    fn channels_view(&self, p: &Palette) -> AnyElement {
        let live: Vec<&AdminChannel> = self.channels.iter().filter(|c| c.deleted_at.is_none()).collect();
        let trash: Vec<&AdminChannel> = self.channels.iter().filter(|c| c.deleted_at.is_some()).collect();
        let mut live_rows = div().flex().flex_col().gap(px(4.));
        for c in &live {
            let mut subs = vec![format!("{} · {}", c.category, type_label(&c.kind))];
            if let Some(t) = c.topic.clone().filter(|t| !t.is_empty()) {
                subs.push(t);
            }
            live_rows = live_rows.child(Self::channel_row(c.id.clone(), channel_icon(&c.kind, 15., p.text_accent), c.name.clone(), subs, Some(c.messages), false, p));
        }
        if live.is_empty() {
            live_rows = live_rows.child(hint("Kanal yok.", p));
        }
        div()
            .flex()
            .flex_col()
            .child(settings_block(format!("Kanallar · {}", live.len()), trash.is_empty(), p).child(live_rows))
            .when(!trash.is_empty(), |s| {
                let mut rows = div().flex().flex_col().gap(px(4.));
                for c in &trash {
                    let sub = format!("{} · {} silindi · {} tarihinde temizlenecek", c.category, c.deleted_at.as_deref().map(day).unwrap_or_else(|| "—".into()), c.purge_at.as_deref().map(day).unwrap_or_else(|| "—".into()));
                    rows = rows.child(Self::channel_row(c.id.clone(), channel_icon(&c.kind, 15., p.text_accent), c.name.clone(), vec![sub], Some(c.messages), true, p));
                }
                s.child(settings_block(format!("Silinmiş kanallar · {}", trash.len()), true, p).child(hint("Silinen kanallar 30 gün saklanır, sonra kalıcı olarak temizlenir.", p)).child(rows))
            })
            .into_any_element()
    }

    fn trash_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(4.));
        for item in &self.trash {
            let it = item.clone();
            let when = format!("{} tarihinde silindi{}", day(&item.deleted_at), item.purge_at.as_deref().map(|d| format!(" · {} tarihinde kendiliğinden kaldırılacak", day_month(d))).unwrap_or_default());
            let busy = self.busy;
            rows = rows.child(
                Self::channel_row(item.id.clone(), text(font_size::ROOT).font_family("Segoe UI Symbol").child("🗑").into_any_element(), item.name.clone(), vec![format!("Kod: {} · Sahip: {}", item.slug, item.owner), when], None, false, p).child(settings_button(SharedString::from(format!("purge-{}", item.id)), "Kalıcı sil", Btn::Danger, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                    if !busy {
                        this.confirm = Some(Confirm::Purge(it.clone()));
                        cx.notify();
                    }
                }))),
            );
        }
        if self.trash.is_empty() {
            // `.admin-empty` (stil tanımı yok: kalıtılan 13,6px metin).
            rows = rows.child(text(font_size::ROOT).text_color(p.text).child("Geri dönüşüm boş."));
        }
        settings_block(format!("Silinen projeler · {}", self.trash.len()), true, p)
            .child(hint("Silinen projeler 30 gün geri dönüşümde bekler. Proje kodu bu süre boyunca kayıtlı kaldığı için aynı adla yeni bir proje açılamaz; kalıcı silmek kodu serbest bırakır ve projenin kanalları, mesajları, panoları geri dönülmez biçimde kaldırılır.", p))
            .child(rows)
            .into_any_element()
    }

    fn organization_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can = true;
        let value = self.org_name.read(cx).value().trim().to_string();
        let save_off = !can || self.busy || value.is_empty() || value == self.org.name;
        let mut col = div().flex().flex_col().child(
            settings_block("Kuruluş adı", self.overview.is_none(), p)
                .child(settings_field("Ad", field_input(&self.org_name, !can, p, window, cx), p))
                .child(hint("Kuruluş, bu P2 kurulumunun hesap kabıdır; hiçbir projeye erişim vermez. Projeler, üyeler ve davetler her projenin kendi yönetim panelindedir.", p))
                .child(div().flex().child(settings_button("org-save", "Kaydet", Btn::Primary, save_off, p).on_click(cx.listener(move |this, _, window, cx| {
                    if !save_off {
                        this.rename_org(window, cx)
                    }
                })))),
        );
        if let Some(o) = &self.overview {
            let rows = [
                ("Adres", o.organization.slug.clone()),
                ("Kimlik", o.organization.id.chars().take(8).collect()),
                ("Kuruluş tarihi", day(&o.organization.created_at)),
                ("Toplam üye", format!("{} kişi", o.members.total)),
                ("Dosya alanı", bytes(o.storage.bytes)),
            ];
            let n = rows.len();
            // direct.css `.channel-details`: p 14 17 18; satırlar 9px, alt çizgi.
            let mut details = div().flex().flex_col().gap(px(1.)).pt(px(14.)).px(px(17.)).pb(px(18.));
            for (i, (k, v)) in rows.into_iter().enumerate() {
                details = details.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .py(px(9.))
                        .when(i + 1 < n, |s| s.border_b_1().border_color(p.border_subtle))
                        .child(text(fs(10.)).text_color(p.text_3).child(k))
                        .child(div().max_w(gpui_kit::relative(0.62)).text_size(fs(11.)).line_height(gpui_kit::relative(1.45)).css_weight(650.).text_color(p.text).child(v)),
                );
            }
            col = col.child(settings_block("Bilgiler", true, p).child(details));
        }
        col.into_any_element()
    }

    fn confirm_dialog(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, body, label) = match self.confirm.as_ref()? {
            Confirm::Remove(m) => ("Üyeyi çıkar".to_string(), format!("{} bu projeden çıkarılacak. Rolleri, grupları ve projedeki sohbet grubu üyelikleri silinir; bir davet koduyla tekrar katılabilir.", m.display_name), "Projeden çıkar"),
            Confirm::Ban(m) => ("Üyeyi yasakla".to_string(), format!("{} bu projeden çıkarılıp yasaklanacak. Yasak kalkana kadar hiçbir davet koduyla geri katılamaz. Yasağı Yasaklılar bölümünden kaldırabilirsin.", m.display_name), "Yasakla"),
            Confirm::Transfer(m) => ("Sahipliği devret".to_string(), format!("{} projenin sahibi olacak; sen yönetici olarak kalırsın ama sahipliği yalnız yeni sahip geri devredebilir.", m.display_name), "Sahipliği devret"),
            Confirm::Sessions(m) => ("Oturumları kapat".to_string(), format!("{} bütün cihazlardaki bütün oturumlarından çıkarılacak ve yeniden giriş yapması gerekecek.", m.display_name), "Oturumları kapat"),
            Confirm::Purge(pr) => (
                "Proje kalıcı olarak silinsin mi?".to_string(),
                format!("“{}” (kod: {}) ve projeye bağlı tüm kanallar, mesajlar, panolar ve görevler geri dönülmez biçimde silinecek. Silindikten sonra aynı kodla yeni bir proje açabilirsin.", pr.name, pr.slug),
                "Kalıcı olarak sil",
            ),
        };
        let ban = matches!(self.confirm, Some(Confirm::Ban(_)));
        let (card, close, cancel, ok) = confirm_modal(title, body, label, true, window, p);
        let card = card.when(ban, |s| s.child(div().px(px(24.)).pb(px(6.)).child(settings_field("Neden (isteğe bağlı)", field_input(&self.ban_reason, false, p, window, cx), p))));
        let dismiss = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.confirm = None;
            cx.notify();
        };
        let close = close.on_click(cx.listener(dismiss));
        // Kapat düğmesi başlığın sağında (modal_head'e `close` verilmediği için mutlak konum).
        let card = card
            .child(modal_foot(p).child(cancel.on_click(cx.listener(dismiss))).child(ok.on_click(cx.listener(|this, _, window, cx| this.run_confirm(window, cx)))))
            .child(div().absolute().top(px(18.)).right(px(16.)).child(close))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("admin-confirm-card", Entrance::PopElastic, std::time::Duration::ZERO);
        Some(
            div()
                .id("admin-confirm")
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
                    this.confirm = None;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }
}

impl Render for AdminPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let (w, h) = modal_size(1080., 760., window);
        let mut nav = match &self.scope {
            AdminScope::Project(pr) => settings_nav(pr.name.clone(), format!("Proje yönetimi · {}", if self.primary_owner() { "Proje sahibi" } else if self.perms.owner { "Sahip" } else { "Yönetici" }), &p),
            AdminScope::Instance => settings_nav("Sunucu yönetimi", format!("{} · Kurulum sahibi", self.org.name), &p),
        };
        for (section, label, sub) in self.visible() {
            nav = nav.child(nav_button(SharedString::from(format!("adm-{label}")), label, Some(sub), self.section == section, &p).on_click(cx.listener(move |this, _, window, cx| this.open(section, window, cx))));
        }
        let content_w = w - 2. - 230. - 40.;
        let body = match self.section {
            Section::Overview => self.overview_view(content_w, &p),
            Section::Members | Section::Roles | Section::Groups => self.team_view(content_w, &p, window, cx),
            Section::Access => self.access_view(&p, cx),
            Section::Invites => self.invites_view(&p, window, cx),
            Section::Bans => self.bans_view(&p, cx),
            Section::Settings => self.settings_view(&p, window, cx),
            Section::Channels => self.channels_view(&p),
            Section::Trash => self.trash_view(&p, cx),
            Section::Organization => self.organization_view(&p, window, cx),
            Section::Sessions => self.sessions_view(&p, cx),
        };
        let title = PROJECT_SECTIONS.iter().chain(INSTANCE_SECTIONS.iter()).find(|s| s.0 == self.section).map(|s| s.1).unwrap_or("Yönetim");
        let access = self.section == Section::Access;
        let scroll = div()
            .id(("admin-scroll", self.section as usize))
            .flex_1()
            .min_h_0()
            .pt(px(16.))
            .px(px(20.))
            .pb(px(30.))
            .flex()
            .flex_col()
            .map(|s| if access { s.overflow_hidden() } else { s.overflow_y_scroll() })
            .child(body)
            .p2_enter(("admin-body", self.section as usize), Entrance::Rise, std::time::Duration::ZERO);
        let card = settings_card("admin-card", w, h, &p)
            .track_focus(&self.focus)
            .child(nav)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(settings_header(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title).into_any_element(), div().id("admin-close").on_click(cx.listener(|_, _, _, cx| cx.emit(AdminEvent::Close))), &p))
                    .child(scroll),
            )
            .p2_enter("admin", Entrance::PopElastic, std::time::Duration::ZERO);
        let confirm = self.confirm_dialog(&p, window, cx);
        settings_layer("admin-scrim", card, &p)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.confirm.is_none() {
                    cx.emit(AdminEvent::Close)
                }
            }))
            .children(confirm)
    }
}
