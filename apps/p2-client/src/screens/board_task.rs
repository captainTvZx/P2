//! Görev ayrıntı paneli — BoardPage.tsx `TaskPanel` + `TaskComments` + `TaskCommentRow`:
//! sağdan açılan panel (`.task-side`, 420px, elastik giriş) ya da ortada geniş pencere (`.task-center`,
//! Ayarlar > Uygulama "Görev ayrıntısı" = `task_view`), başlık, öncelik/puan/sprint/bitiş çipleri,
//! atanan kişi seçimi, sütun düğmeleri, sprint seçimi, açıklama, ayrıntı listesi, konuşma (yorumlar,
//! tepkiler, sağ tık menüsü, düzenle/sil) ve yorum yazma alanı (kalın, italik, bağlantı, emoji, etiketle).

use super::*;

impl BoardPage {
    /// Görev ayrıntısı geniş (ortada) mı açılır: Ayarlar > Uygulama `task_view`.
    /// Not: pencere kökü gpui-kit `Root` olduğundan `window.root::<ClientApp>()` hep boş dönüyordu;
    /// düğme hiçbir şey yapmıyor, ayar da okunamıyordu. Uygulama tanıtıcısı `BoardPage::new` ile gelir.
    fn task_view_center(&self, cx: &gpui_kit::App) -> bool {
        self.app.upgrade().is_some_and(|app| app.read(cx).settings.app.task_view == "center")
    }

    /// Electron `updateSettings({taskView: genis ? 'panel' : 'center'})`: tercih kalıcı ayara yazılır.
    fn toggle_task_view(&mut self, cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, _| {
            app.settings.app.task_view = if app.settings.app.task_view == "center" { "panel".into() } else { "center".into() };
            crate::trace(&format!("pano: görev görünümü {}", app.settings.app.task_view));
            p2_client_core::store::save_settings(&app.settings);
        });
        cx.notify();
    }

    pub(super) fn send_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ctx), Some(id)) = (self.ctx(), self.selected.clone()) else { return };
        // `attachments.appendTo(draft.trim())`: metin + her ek için bir `[[p2-file|…]]` satırı.
        let draft = self.comment.read(cx).value().trim().to_string();
        let text = std::iter::once(draft).chain(self.comment_files.iter().map(p2_client_core::attachment_marker)).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
        if text.is_empty() || self.sending || self.comment_uploading {
            return;
        }
        self.sending = true;
        self.comment_files.clear();
        self.comment_upload_error = None;
        self.comment.update(cx, |i, cx| i.set_value("", window, cx));
        cx.notify();
        task::run(window, cx, move || ctx.api.add_task_comment(&ctx.org, &ctx.project, &ctx.board, &id, &text), |this, result, _, cx| {
            this.sending = false;
            match result {
                Ok(c) => {
                    if !this.comments.iter().any(|x| x.id == c.id) {
                        this.comments.push(c);
                    }
                    crate::trace("pano: yorum gönderildi");
                }
                Err(e) => this.fail(e, cx),
            }
            cx.notify();
        });
    }

    fn comment_request(&mut self, method: reqwest::Method, comment: String, suffix: &'static str, body: Option<Value>, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ctx), Some(task_id)) = (self.ctx(), self.selected.clone()) else { return };
        let path = format!("/organizations/{}/projects/{}/boards/{}/tasks/{task_id}/comments/{comment}{suffix}", ctx.org, ctx.project, ctx.board);
        let m = method.clone();
        task::run(window, cx, move || ctx.api.request(m, &path, body, true), move |this, result, _, cx| {
            match result {
                Ok(v) => match suffix {
                    "/reactions" => {
                        if let Ok(rx) = serde_json::from_value::<Vec<p2_client_core::Reaction>>(v) {
                            if let Some(c) = this.comments.iter_mut().find(|c| c.id == comment) {
                                c.reactions = rx;
                            }
                        }
                    }
                    _ if method == reqwest::Method::DELETE => this.comments.retain(|c| c.id != comment),
                    _ => {
                        if let Ok(updated) = serde_json::from_value::<TaskComment>(v) {
                            if let Some(c) = this.comments.iter_mut().find(|c| c.id == comment) {
                                let reactions = std::mem::take(&mut c.reactions);
                                *c = updated;
                                c.reactions = reactions;
                            }
                        }
                    }
                },
                Err(e) => this.fail(e, cx),
            }
            cx.notify();
        });
    }

    fn save_comment_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, state)) = self.comment_edit.take() else { return };
        let text = state.read(cx).value().trim().to_string();
        let old = self.comments.iter().find(|c| c.id == id).map(|c| c.content.clone()).unwrap_or_default();
        if !text.is_empty() && text != old {
            self.comment_request(reqwest::Method::PATCH, id, "", Some(json!({ "content": text })), window, cx);
        }
        cx.notify();
    }

    fn wrap_comment(&mut self, marker: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.comment.update(cx, |s, cx| {
            let value = s.value().to_string();
            let range = s.selected_range();
            let (a, b) = (range.start.min(value.len()), range.end.min(value.len()));
            s.set_value(format!("{}{marker}{}{marker}{}", &value[..a], &value[a..b], &value[b..]), window, cx);
            let end = a + marker.len() + (b - a);
            s.set_selected_range(end..end, cx);
            s.focus(window, cx);
        });
    }

    fn insert_comment(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.comment.update(cx, |s, cx| {
            s.insert(text, window, cx);
            s.focus(window, cx);
        });
    }

    /// `.board-editor-tools button` (28 × 28, 15px ikon).
    fn editor_tool(id: &'static str, icon: &'static str, tip: &'static str, active: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
        let (soft, strong) = (p.accent_soft, p.text_strong);
        div()
            .id(id)
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .map(|s| if active { s.bg(p.accent_soft_2).text_color(p.text_accent) } else { s.text_color(p.text_3).hover(move |s| s.bg(soft).text_color(strong)) })
            .child(p2_ui::icons::icon(icon).stroke(1.9).el().size(px(15.)).text_color(if active { p.text_accent } else { p.text_3 }))
            .tooltip(move |window, cx| p2_ui::controls::tooltip(tip, window, cx))
    }

    fn comment_row(&self, c: &TaskComment, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let author = self.member(Some(c.author_id.as_str()));
        let name = author.map(|m| m.display_name.clone()).unwrap_or_else(|| "Bilinmeyen".into());
        let edited = c.updated_at.as_deref().is_some_and(|u| u != c.created_at);
        let editing = self.comment_edit.as_ref().filter(|e| e.0 == c.id).map(|e| e.1.clone());
        let mid = c.id.clone();
        let body: AnyElement = match editing {
            Some(state) => div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().min_h(px(64.)).py(px(8.)).px(px(10.)).border_1().border_color(p.border_accent).rounded(radius::SM).bg(p.bg_3).text_size(fs(12.)).child(gpui_kit::component::input::Textarea::new(&state).appearance(false).p(px(0.)).text_size(fs(12.)).text_color(p.text)))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(6.))
                        .child(sized_button("ce-cancel", "Vazgeç", Btn::Plain, false, 26., 11., fs(11.), p).on_click(cx.listener(|this, _, _, cx| {
                            this.comment_edit = None;
                            cx.notify();
                        })))
                        .child(sized_button("ce-save", "Kaydet", Btn::Primary, false, 26., 11., fs(11.), p).on_click(cx.listener(|this, _, window, cx| this.save_comment_edit(window, cx)))),
                )
                .into_any_element(),
            None => p2_ui::rich::render_message(&c.content, p),
        };
        let mut reactions = div().mt(px(4.)).flex().flex_wrap().gap(px(5.));
        for r in &c.reactions {
            let (border, bg, fg) = if r.reacted { (p.border_accent, p.accent_soft, p.text_accent) } else { (p.border, p.bg_3, p.text_2) };
            let (cid, emoji) = (c.id.clone(), r.emoji.clone());
            reactions = reactions.child(
                div()
                    .id(SharedString::from(format!("crx-{}-{}", c.id, r.emoji)))
                    .h(px(25.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .border_1()
                    .border_color(border)
                    .rounded(px(13.))
                    .bg(bg)
                    .text_color(fg)
                    .cursor_pointer()
                    .child(div().font_family(p2_ui::tokens::FONT_EMOJI).text_size(fs(13.)).line_height(fs(13.)).child(r.emoji.clone()))
                    .child(div().text_size(fs(10.)).css_weight(750.).child(r.count.to_string()))
                    .on_click(cx.listener(move |this, _, window, cx| this.comment_request(reqwest::Method::POST, cid.clone(), "/reactions", Some(json!({ "emoji": emoji })), window, cx))),
            );
        }
        div()
            .id(SharedString::from(format!("comment-{}", c.id)))
            .relative()
            .flex()
            .gap(px(9.))
            .child(board_avatar(author, 26., p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .mb(px(2.))
                            .flex()
                            .items_baseline()
                            .gap(px(7.))
                            .child(div().text_size(fs(11.5)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(name))
                            .child(div().text_size(fs(10.)).text_color(p.text_3).child(comment_time(&c.created_at)))
                            .when(edited, |s| s.child(div().text_size(fs(9.5)).text_color(p.text_3).child("düzenlendi"))),
                    )
                    .child(div().text_size(fs(12.)).text_color(p.text).child(body))
                    .when(!c.reactions.is_empty(), |s| s.child(reactions)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.close_overlays();
                    this.menu_epoch += 1;
                    this.comment_menu = Some((mid.clone(), ev.position));
                    cx.notify();
                }),
            )
            .p2_enter(SharedString::from(format!("comment-in-{}", c.id)), Entrance::Rise, Duration::ZERO)
            .into_any_element()
    }

    /// Yorumun sağ tık menüsü (MessageTools: tepki, kopyala, düzenle, sil).
    pub(super) fn comment_menu_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, at) = self.comment_menu.clone()?;
        let c = self.comments.iter().find(|c| c.id == id)?.clone();
        let own = c.author_id == self.me;
        let fresh = local(&c.created_at).is_some_and(|d| (chrono::Local::now() - d).num_minutes() <= 120);
        let editable = own && fresh;
        let deletable = editable || self.can_manage;
        let quick = ["👍", "❤️", "😂", "🎉", "👀"];
        let h = 6. * 2. + 2. + 40. + 34. * (1 + editable as usize + deletable as usize) as f32 + 11.;
        let (x, y) = place_menu(f32::from(at.x), f32::from(at.y), f32::from(at.y), false, 230., h, window);
        let mut react = div().h(px(40.)).px(px(4.)).flex().items_center().justify_between();
        for e in quick {
            let cid = c.id.clone();
            let soft = p.accent_soft;
            react = react.child(
                div()
                    .id(SharedString::from(format!("cm-rx-{e}")))
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .font_family(p2_ui::tokens::FONT_EMOJI)
                    .text_size(fs(17.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft))
                    .child(e)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.comment_menu = None;
                        this.comment_request(reqwest::Method::POST, cid.clone(), "/reactions", Some(json!({ "emoji": e })), window, cx);
                    })),
            );
        }
        let content = c.content.clone();
        let mut menu = menu_panel("comment-menu", 230., 6., p).child(react).child(menu_hr(p)).child(menu_item("cm-copy", "⧉", "Metni kopyala", Tone::Normal, None, 34., p).on_click(cx.listener(move |this, _, _, cx| {
            this.comment_menu = None;
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(content.clone()));
            cx.notify();
        })));
        if editable {
            let (cid, text) = (c.id.clone(), c.content.clone());
            menu = menu.child(menu_item("cm-edit", "✎", "Yorumu düzenle", Tone::Normal, None, 34., p).on_click(cx.listener(move |this, _, window, cx| {
                this.comment_menu = None;
                let state = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8).submit_on_enter(true));
                state.update(cx, |s, cx| {
                    s.set_value(text.clone(), window, cx);
                    s.focus(window, cx);
                });
                let sub = cx.subscribe_in(&state, window, |this, _, ev: &gpui_kit::component::input::InputEvent, window, cx| {
                    if let gpui_kit::component::input::InputEvent::PressEnter { .. } = ev {
                        this.save_comment_edit(window, cx);
                    }
                });
                this._subs.push(sub);
                this.comment_edit = Some((cid.clone(), state));
                cx.notify();
            })));
        }
        if deletable {
            let cid = c.id.clone();
            menu = menu.child(menu_item("cm-delete", "×", "Yorumu sil", Tone::Danger, None, 34., p).on_click(cx.listener(move |this, _, window, cx| {
                this.comment_menu = None;
                this.comment_request(reqwest::Method::DELETE, cid.clone(), "", None, window, cx);
            })));
        }
        let menu = menu.p2_enter(("comment-menu", self.menu_epoch), Entrance::ScaleIn, Duration::ZERO);
        Some(Self::layer("comment-menu-layer", div().absolute().left(px(x)).top(px(y)).child(menu).into_any_element(), window, cx))
    }

    /// Emoji düğmesi: tam EmojiPanel'i düğmenin altında (sığmazsa üstünde) açar/kapatır.
    pub(super) fn toggle_comment_emoji(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.comment_emoji.take().is_some() {
            cx.notify();
            return;
        }
        let panel = super::super::chat::emoji::EmojiPanel::new(window, cx);
        let sub = cx.subscribe(&panel.search, |this, input, event: &gpui_kit::component::input::InputEvent, cx| {
            if let gpui_kit::component::input::InputEvent::Change = event {
                let q = input.read(cx).value().to_string();
                if let Some(p) = this.comment_emoji.as_mut() {
                    p.set_query(q);
                }
                cx.notify();
            }
        });
        self._subs.push(sub);
        self.comment_emoji = Some(panel);
        cx.notify();
    }

    /// `MessageMenuPopup.reaction-picker-popup.task-comment-emoji-menu` + tam `EmojiPanel`
    /// (`anchorFrom(düğme)`: x = sol, y = alt + 6, yukarı = üst − 6).
    fn emoji_layer(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let panel = self.comment_emoji.as_ref()?;
        let Some(b) = self.emoji_anchor.get() else {
            // Düğme henüz çizilmedi (ilk kare): sınırlar bir sonraki karede gelir.
            window.request_animation_frame();
            return None;
        };
        let (bx, top, bottom) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.origin.y + b.size.height));
        let (left, y) = place_menu(bx, bottom + 6., top - 6., false, 330., 354., window);
        let who = cx.entity().downgrade();
        let pick: super::super::chat::emoji::Pick = std::rc::Rc::new(move |e, window, cx| {
            let _ = who.update(cx, |this, cx| {
                if let Some(panel) = this.comment_emoji.as_mut() {
                    panel.chose(&e);
                }
                this.comment_emoji = None;
                this.insert_comment(&e, window, cx);
                cx.notify();
            });
        });
        let card = div()
            .id("task-comment-emoji-menu")
            .w(px(330.))
            .overflow_hidden()
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::LG)
            .bg(p.bg_2)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(panel.render(p, pick))
            .p2_enter("task-emoji", Entrance::ScaleIn, Duration::ZERO);
        Some(Self::layer("task-emoji-layer", div().absolute().left(px(left)).top(px(y)).child(card).into_any_element(), window, cx))
    }

    /// BoardPage.tsx `mentionItems`: taslak `(?:^|\s)@([\w.-]*)$` ile bitiyorsa üyelerden en çok 8 öneri
    /// (ad, `@kullanıcı`, baş harfler).
    pub(super) fn mention_items(&self, cx: &gpui_kit::App) -> Vec<(String, String, String)> {
        let draft = self.comment.read(cx).value().to_string();
        let Some(at) = draft.rfind('@') else { return Vec::new() };
        let query = &draft[at + 1..];
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-';
        if !query.chars().all(word) || !(at == 0 || draft[..at].ends_with(char::is_whitespace)) {
            return Vec::new();
        }
        // `toLocaleLowerCase('tr-TR')`: I → ı, İ → i.
        let lower = |s: &str| s.replace('I', "ı").replace('İ', "i").to_lowercase();
        let q = lower(query);
        self.members
            .iter()
            .filter(|m| !m.username.is_empty() && lower(&format!("{} {}", m.display_name, m.username)).contains(&q))
            .take(8)
            .map(|m| (m.display_name.clone(), format!("@{}", m.username), p2_ui::components::user_initials(&m.display_name)))
            .collect()
    }

    /// `pickMention`: sondaki `@sorgu` yerine `@kullanıcı ` yazar.
    fn pick_mention(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mention_ix = 0;
        self.comment.update(cx, |s, cx| {
            let draft = s.value().to_string();
            let at = draft.rfind('@').unwrap_or(draft.len());
            s.set_value(format!("{}{value} ", &draft[..at]), window, cx);
            let end = s.value().len();
            s.set_selected_range(end..end, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter/Tab: öneri açıksa etkin olanı ekler (true döner).
    pub(super) fn pick_active_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let items = self.mention_items(cx);
        let Some(item) = items.get(self.mention_ix.min(items.len().saturating_sub(1))).cloned() else { return false };
        self.pick_mention(item.1, window, cx);
        true
    }

    /// `ComposerSuggestions` (`.composer-suggestions`): yazma alanının üstünde, sol 8px, alt 100% + 7px.
    fn mention_popup(&self, items: &[(String, String, String)], p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.mention_ix.min(items.len().saturating_sub(1));
        let mut pop = div()
            .id("task-mentions")
            .w(px(330.))
            .max_h(px(350.))
            .overflow_y_scroll()
            .p(px(6.))
            .flex()
            .flex_col()
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(11.))
            .bg(p.glass.background())
            .backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .child(
                div()
                    .h(px(31.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_color(p.text_accent)
                    .child(div().text_size(font_size::XXS).css_weight(700.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).child(super::super::chat::view::tr_upper("Etiketler")))
                    .child(div().text_size(fs(8.)).text_color(p.text_4).child("↑↓ ile seç, Enter ile ekle")),
            );
        for (i, (label, handle, avatar)) in items.iter().enumerate() {
            let on = i == active;
            let (soft, soft2, accent_hover) = (p.accent_soft, p.accent_soft_2, p.accent_hover);
            let value = handle.clone();
            pop = pop.child(
                div()
                    .id(SharedString::from(format!("tm-{i}")))
                    .w_full()
                    .flex_none()
                    .h(px(43.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(6.))
                    .text_color(if on { p.text_strong } else { p.text })
                    .cursor_pointer()
                    .when(on, |s| s.bg(soft2).shadow(shadows(&[Shadow { x: 2., y: 0., blur: 0., spread: 0., color: accent_hover, inset: true }])))
                    .when(!on, |s| s.hover(move |s| s.bg(soft)))
                    .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                        if *over && this.mention_ix != i {
                            this.mention_ix = i;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.pick_mention(value.clone(), window, cx);
                        }),
                    )
                    .child(div().w(px(34.)).child(div().size(px(27.)).flex().items_center().justify_center().rounded(px(8.)).bg(p.bg_selected).text_color(p.text_strong).text_size(fs(10.)).css_weight(800.).child(avatar.clone())))
                    .child(div().flex().flex_col().gap(px(3.)).child(div().text_size(fs(11.)).css_weight(700.).child(label.clone())).child(div().text_size(fs(9.)).text_color(p.text_3).child(handle.clone()))),
            );
        }
        // `transform-origin: bottom left` + `p2-scale-in`.
        div().absolute().left(px(8.)).bottom_full().mb(px(7.)).child(pop.p2_enter("task-mentions", Entrance::ScaleInFrom { ox: 0., oy: 1. }, Duration::ZERO)).into_any_element()
    }

    /// `AttachmentPicker`: dosya seçtirir, her birini `/uploads`'a yükler (useAttachments `add`).
    fn attach_comment_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Dosya ekle".into()) });
        let api = self.ctx_api.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            if paths.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.comment_uploading = true;
                this.comment_upload_error = None;
                cx.notify();
            });
            let result = cx
                .background_spawn(async move {
                    let mut out = Vec::new();
                    for path in paths {
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dosya".into());
                        let data = std::fs::read(&path).map_err(|_| format!("{name} yüklenemedi."))?;
                        let file = api.upload_file(&name, &data).map_err(|e| if e.message.is_empty() { format!("{name} yüklenemedi.") } else { e.message })?;
                        out.push(file);
                    }
                    Ok::<_, String>(out)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.comment_uploading = false;
                match result {
                    Ok(files) => {
                        crate::trace(&format!("pano: {} ek yüklendi", files.len()));
                        this.comment_files.extend(files);
                    }
                    Err(e) => this.comment_upload_error = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `AttachmentQueue` (`.composer-attachments`): yüklenen ekler, "Dosya yükleniyor…", hata.
    fn attachment_queue(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.comment_files.is_empty() && !self.comment_uploading && self.comment_upload_error.is_none() {
            return None;
        }
        let mut q = div().flex().flex_wrap().gap(px(7.)).pt(px(8.)).px(px(10.)).pb(px(2.));
        for (ix, f) in self.comment_files.iter().enumerate() {
            let ext = f.name.rsplit('.').next().filter(|e| *e != f.name).unwrap_or("DSY");
            let kind = if f.mime == "application/pdf" { "PDF".to_string() } else if f.mime.starts_with("audio/") { "SES".into() } else { ext.chars().take(4).collect::<String>().to_uppercase() };
            let (danger_soft, danger_text) = (p.danger_soft, p.danger_text);
            q = q.child(
                div()
                    .max_w(px(260.))
                    .h(px(38.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .child(div().flex_none().size(px(34.)).flex().items_center().justify_center().border_1().border_color(p.border_subtle).rounded(px(8.)).bg(p.bg_hover).text_color(p.text_2).text_size(fs(9.)).css_weight(750.).child(kind))
                    .child(div().min_w_0().truncate().text_color(p.text).text_size(fs(10.)).css_weight(700.).child(f.name.clone()))
                    .child(div().flex_none().text_color(p.text_3).text_size(fs(8.)).child(super::super::chat::extras::format_size(f.size)))
                    .child(
                        div()
                            .id(SharedString::from(format!("cq-x-{ix}")))
                            .flex_none()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(danger_soft).text_color(danger_text))
                            .child("×")
                            .tooltip(|window, cx| p2_ui::controls::tooltip("Kaldır", window, cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if ix < this.comment_files.len() {
                                    this.comment_files.remove(ix);
                                }
                                cx.notify();
                            })),
                    )
                    .p2_enter(("cq", ix), Entrance::ScaleIn, Duration::ZERO),
            );
        }
        if self.comment_uploading {
            q = q.child(div().my(px(5.)).text_color(p.text_3).text_size(fs(10.)).child("Dosya yükleniyor…"));
        }
        if let Some(e) = &self.comment_upload_error {
            q = q.child(div().my(px(5.)).text_color(p.danger_text).text_size(fs(10.)).child(e.clone()));
        }
        Some(q.into_any_element())
    }

    pub(super) fn task_panel(&self, task: &Task, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let center = self.task_view_center(cx);
        let can_edit = self.can_edit(task);
        let owner = self.member(task.assignee_id.as_deref());
        let author = self.member(Some(task.created_by_id.as_str()));
        let sprints = self.content.as_ref().map(|c| c.sprints.clone()).unwrap_or_default();
        let sprint = sprints.iter().find(|s| Some(&s.id) == task.sprint_id.as_ref()).cloned();
        let late = is_late(task);
        let h4 = |t: &str| div().text_size(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(t));
        let block = |title: &str, body: AnyElement| div().flex().flex_col().gap(px(8.)).child(h4(title)).child(body);

        // Başlık çubuğu (46px)
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let tool = |id: &'static str, icon: &'static str, tip: &'static str| {
            div()
                .id(id)
                .size(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::SM)
                .text_color(p.text_3)
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child(p2_ui::icons::icon(icon).stroke(1.9).el().size(px(16.)).text_color(p.text_3))
                .tooltip(move |window, cx| p2_ui::controls::tooltip(tip, window, cx))
        };
        let header = div()
            .flex_none()
            .h(px(46.))
            .pl(px(16.))
            .pr(px(8.))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(p.border_subtle)
            .child(div().flex().items_baseline().gap(px(9.)).child(mono(format!("#{}", task.number), 11., p.text_accent)).child(div().text_size(fs(12.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(column_label(task.column()))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(tool("task-wide", if center { "collapse" } else { "expand" }, if center { "Sağ panele al" } else { "Ortada geniş aç" }).on_click(cx.listener(|this, _, _, cx| this.toggle_task_view(cx))))
                    .child(tool("task-close", "close", "Kapat").on_click(cx.listener(|this, _, _, cx| {
                        this.selected = None;
                        cx.notify();
                    }))),
            );

        // Çipler
        let chip = |text: String, late: bool| div().flex_none().px(px(9.)).py(px(3.)).rounded(px(99.)).bg(if late { p.danger_soft } else { p.bg_hover }).text_color(if late { p.danger_text } else { p.text_2 }).text_size(fs(10.5)).css_weight(650.).child(text);
        let chips = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .child(priority_chip(&task.priority, p))
            .when_some(task.points, |s, pts| s.child(chip(format!("{pts} puan"), false)))
            .child(chip(sprint.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Backlog".into()), false))
            .when_some(task.due_at.clone(), |s, d| s.child(chip(day_text(&d), late)));

        // Atanan
        let tid = task.id.clone();
        let person: AnyElement = if can_edit {
            let opts: Vec<(String, String)> = std::iter::once((String::new(), "Kimseye atanmadı".to_string())).chain(self.members.iter().map(|m| (m.id.clone(), m.display_name.clone()))).collect();
            let cur = task.assignee_id.clone().unwrap_or_default();
            let pick = move |this: &mut Self, v: String, window: &mut Window, cx: &mut Context<Self>| this.update_task(tid.clone(), json!({ "assigneeId": v }), None, window, cx);
            div().flex().items_center().gap(px(10.)).child(board_avatar(owner, 30., p)).child(div().flex_1().child(self.wide_select(OpenSelect::PanelAssignee, "panel-who", &opts, &cur, p, cx, pick))).into_any_element()
        } else {
            div().flex().items_center().gap(px(10.)).child(board_avatar(owner, 30., p)).child(div().text_size(fs(12.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text).child(owner.map(|m| m.display_name.clone()).unwrap_or_else(|| "Kimseye atanmadı".into()))).into_any_element()
        };

        // Sütun düğmeleri (2 × 2)
        let mut cols = div().flex().flex_wrap().gap(px(6.));
        for status in BOARD_COLUMNS {
            let on = task.column() == status;
            let tid = task.id.clone();
            let (hbg, strong) = (p.bg_hover, p.text_strong);
            let disabled = !can_edit || self.busy;
            cols = cols.child(
                div()
                    .id(SharedString::from(format!("panel-col-{status}")))
                    .w(px(if center { 200. } else { 175. }))
                    .flex_grow(1.)
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .rounded(radius::SM)
                    .text_size(fs(11.))
                    .css_weight(650.)
                    .when(disabled, |s| s.opacity(0.5))
                    .map(|s| if on { s.border_color(gpui_kit::transparent_black()).bg(p.accent_grad.background()).text_color(p.text_on_accent).shadow(shadows(&[p.accent_glow_strong])) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text).when(!disabled, |s| s.cursor_pointer().hover(move |s| s.bg(hbg).text_color(strong))) })
                    .child(column_label(status))
                    .when(!disabled, |s| s.on_click(cx.listener(move |this, _, window, cx| this.update_task(tid.clone(), json!({ "status": status }), None, window, cx)))),
            );
        }

        // Sprint
        let sprint_el: AnyElement = if can_edit {
            let opts: Vec<(String, String)> = std::iter::once((String::new(), "Backlog".to_string())).chain(sprints.iter().map(|s| (s.id.clone(), format!("{}{}", s.name, if s.status == "ACTIVE" { " · aktif" } else { "" })))).collect();
            let cur = task.sprint_id.clone().unwrap_or_default();
            let (tid, number) = (task.id.clone(), task.number);
            let names: Vec<(String, String)> = sprints.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
            let pick = move |this: &mut Self, v: String, window: &mut Window, cx: &mut Context<Self>| {
                let note = if v.is_empty() { format!("#{number} backlog’a döndü.") } else { format!("#{number} “{}” sprint’ine alındı.", names.iter().find(|n| n.0 == v).map(|n| n.1.as_str()).unwrap_or("sprint")) };
                let patch = if v.is_empty() { json!({ "sprintId": null }) } else { json!({ "sprintId": v }) };
                this.update_task(tid.clone(), patch, Some(note), window, cx);
            };
            self.wide_select(OpenSelect::PanelSprint, "panel-sprint", &opts, &cur, p, cx, pick)
        } else {
            div().text_size(fs(12.)).text_color(p.text).child(sprint.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| "Backlog".into())).into_any_element()
        };

        let description: AnyElement = match task.description.as_deref().filter(|d| !d.is_empty()) {
            // `.task-panel-text` içindeki `<p>` 1em (10,2px) üst/alt kenar boşluğu taşır (ölçüm: blok 36,7 = 10,2 + 16,3 + 10,2).
            Some(d) => div().py(fs(12.)).text_size(fs(12.)).line_height(gpui_kit::relative(1.6)).text_color(p.text).child(p2_ui::rich::render_message(d, p)).into_any_element(),
            None => div().text_size(fs(11.5)).italic().text_color(p.text_3).child("Açıklama yazılmamış.").into_any_element(),
        };
        // `.task-panel-meta` dt/dd `line-height: normal` (11px satır); etiket satırı çiplerle 20px.
        let meta_row = |dt: &str, dd: AnyElement| div().flex().gap(px(10.)).child(div().w(px(74.)).flex_none().text_size(fs(11.)).line_height(p2_ui::text::normal_line_height(fs(11.))).text_color(p.text_3).child(dt.to_string())).child(div().flex_1().min_w_0().text_size(fs(11.5)).line_height(p2_ui::text::normal_line_height(fs(11.5))).text_color(p.text).child(dd));
        let meta = div()
            .flex()
            .flex_col()
            .gap(px(7.))
            .child(meta_row("Tür", div().child(p2_client_core::kind_label(task.kind())).into_any_element()))
            .when_some(label_chips(task, 10, true, p), |s, chips| s.child(meta_row("Etiketler", chips.into_any_element())))
            .child(meta_row("Açan", div().child(author.map(|m| m.display_name.clone()).unwrap_or_else(|| "—".into())).into_any_element()))
            .child(meta_row("Bitiş", div().when(late, |s| s.text_color(p.danger_text)).child(task.due_at.as_deref().map(long_day).unwrap_or_else(|| "—".into())).into_any_element()));

        // Konuşma
        let mut list = div().flex().flex_col().gap(px(10.));
        for c in &self.comments {
            list = list.child(self.comment_row(c, p, cx));
        }
        if self.comments.is_empty() {
            list = list.child(div().text_size(fs(11.5)).italic().text_color(p.text_3).child("Henüz yorum yok. İlk sözü sen söyle."));
        }
        let draft_empty = self.comment.read(cx).value().trim().is_empty() && self.comment_files.is_empty();
        let send_off = self.sending || self.comment_uploading || draft_empty;
        let mentions = self.mention_items(cx);
        let mention_count = mentions.len();
        let anchor = self.emoji_anchor.clone();
        let focused = gpui_kit::Focusable::focus_handle(self.comment.read(cx), cx).is_focused(window);
        let composer = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(8.))
            .border_1()
            .rounded(radius::MD)
            .bg(p.bg_3)
            .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
            // Öneri listesinde ↑/↓/Tab (BoardPage.tsx `onKeyDown`); Enter abonelikte işlenir.
            .capture_key_down(cx.listener(move |this, ev: &gpui_kit::KeyDownEvent, window, cx| {
                if mention_count == 0 {
                    return;
                }
                match ev.keystroke.key.as_str() {
                    "down" => this.mention_ix = (this.mention_ix.min(mention_count - 1) + 1) % mention_count,
                    "up" => this.mention_ix = (this.mention_ix.min(mention_count - 1) + mention_count - 1) % mention_count,
                    "tab" => {
                        this.pick_active_mention(window, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .when(mention_count > 0, |s| s.child(self.mention_popup(&mentions, p, cx)))
            .child(div().min_h(px(58.)).py(px(4.)).ml(px(-2.)).text_size(fs(12.)).line_height(gpui_kit::relative(1.6)).text_color(p.text)// XSmall: Textarea iç boşluğu 4px yatay, 0 dikey (varsayılan boyut ~20px içeriden başlatıyordu).
            .child(gpui_kit::component::Sizable::with_size(gpui_kit::component::input::Textarea::new(&self.comment).appearance(false), gpui_kit::component::Size::XSmall).text_size(fs(12.)).line_height(px(16.32)).text_color(p.text)))
            .children(self.attachment_queue(p, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(Self::editor_tool("ct-bold", "composer:bold", "Kalın (Ctrl+B)", false, p).on_click(cx.listener(|this, _, window, cx| this.wrap_comment("**", window, cx))))
                    .child(Self::editor_tool("ct-italic", "composer:italic", "İtalik (Ctrl+I)", false, p).on_click(cx.listener(|this, _, window, cx| this.wrap_comment("_", window, cx))))
                    .child(Self::editor_tool("ct-link", "composer:link", "Bağlantı ekle", false, p).on_click(cx.listener(|this, _, window, cx| this.insert_comment("[bağlantı](https://)", window, cx))))
                    .child(
                        Self::editor_tool("ct-emoji", "composer:emoji", "Emoji", self.comment_emoji.is_some(), p)
                            // Düğmenin pencere sınırları: panel `anchorFrom(getBoundingClientRect())` ile konumlanır.
                            .child(gpui_kit::canvas(move |bounds, _, _| anchor.set(Some(bounds)), |_, _, _, _| {}).absolute().size_full())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_comment_emoji(window, cx))),
                    )
                    .child(Self::editor_tool("ct-mention", "composer:mention", "Birini etiketle", false, p).on_click(cx.listener(|this, _, window, cx| this.insert_comment("@", window, cx))))
                    .child(Self::editor_tool("ct-attach", "composer:plus", "Dosya ekle", false, p).on_click(cx.listener(|this, _, window, cx| this.attach_comment_files(window, cx))))
                    .child(div().flex_1())
                    .when_some(self.comment_upload_error.clone(), |s, e| s.child(div().text_size(fs(10.5)).text_color(p.danger_text).child(e)))
                    .child(sized_button("ct-send", if self.sending { "…" } else { "Gönder" }, Btn::Primary, send_off, 26., 12., fs(11.), p).when(!send_off, |s| s.on_click(cx.listener(|this, _, window, cx| this.send_comment(window, cx))))),
            );
        let comments = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(h4(&if self.comments.is_empty() { "Konuşma".to_string() } else { format!("Konuşma · {}", self.comments.len()) }))
            .child(list)
            .child(composer);

        let body = div()
            .id("task-panel-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(div().text_size(fs(16.)).line_height(gpui_kit::relative(1.35)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(task.title.clone()))
            .child(chips)
            .child(block("Atanan", person))
            .child(block("Sütun", cols.into_any_element()))
            .child(block("Sprint", sprint_el))
            .child(block("Açıklama", description))
            .child(block("Ayrıntı", meta.into_any_element()))
            .child(comments);

        let footer = can_edit.then(|| {
            let (edit, del) = (task.clone(), task.clone());
            div()
                .flex_none()
                .py(px(12.))
                .px(px(16.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_t_1()
                .border_color(p.border_subtle)
                .child(board_button("task-del", "Sil", Btn::Danger, self.busy, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.confirm = Some(Confirm::Task(del.clone()));
                    cx.notify();
                })))
                .child(div().flex_1())
                .child(board_button("task-edit", "Düzenle", Btn::Primary, self.busy, p).on_click(cx.listener(move |this, _, window, cx| this.open_editor(Some(edit.clone()), "TODO", window, cx))))
        });

        let panel = div()
            .id("task-panel")
            .flex()
            .flex_col()
            .bg(p.glass_strong.background())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body)
            .children(footer);
        let menu: Vec<AnyElement> = self.comment_menu_view(p, window, cx).into_iter().chain(self.emoji_layer(p, window, cx)).collect();
        if center {
            let vw = f32::from(window.viewport_size().width);
            let vh = f32::from(window.viewport_size().height);
            let layer = div()
                .id("task-center-layer")
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.selected = None;
                        cx.notify();
                    }),
                )
                .child(
                    panel
                        .w(px(860f32.min(vw - 64.)))
                        .max_h(px(vh - 96.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(radius::XL)
                        .shadow(p.shadow_lg())
                        .p2_enter(("task-center", task.number as usize), Entrance::PopElastic, Duration::ZERO),
                )
                .children(menu);
            window_cover(layer, window)
                .into_any_element()
        } else {
            let vw = f32::from(window.viewport_size().width);
            // Gölge başlık çubuğunun üstüne taşmasın (Electron'da başlık çubuğu z 1000): kap 40px sol payla kırpılır.
            let side = div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .pl(px(40.))
                .overflow_hidden()
                .child(
                    panel
                        .h_full()
                        .w(px(420f32.min(vw * 0.92)))
                        .border_l_1()
                        .border_color(p.border_strong)
                        .shadow(shadows(&[Shadow::new(-8., 0., 24., 0., p2_ui::tokens::css_rgba(0, 0, 0, 0.4))]))
                        .with_animation(("task-panel-in", task.number as usize), Animation::new(p2_ui::motion::M_SLOW).with_easing(|t| p2_ui::motion::EASE_ELASTIC.ease(t)), |el, t| el.opacity(t.clamp(0., 1.)).translate(28. * (1. - t), 0.)),
                );
            div().absolute().inset_0().child(side).children(menu).into_any_element()
        }
    }

    /// Ayrıntı panelindeki tam genişlikte `select` (`.task-panel-person select`: h 30, p 0 9, `11.5px` normal).
    fn wide_select(&self, which: OpenSelect, id: &'static str, options: &[(String, String)], current: &str, p: &Palette, cx: &mut Context<Self>, pick: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static) -> AnyElement {
        let labels: Vec<String> = options.iter().map(|o| o.1.clone()).collect();
        let selected = options.iter().position(|o| o.0 == current).unwrap_or(0);
        let open = self.open_select == Some(which);
        let field = native_select(SharedString::from(format!("sel-{id}")), labels.get(selected).cloned().unwrap_or_default(), 0., 30., 9., fs(11.5), 400., open, p)
            .w_full()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                let was = this.open_select == Some(which);
                this.close_overlays();
                this.open_select = (!was).then_some(which);
                cx.notify();
            }));
        let mut wrap = div().relative().w_full().child(field);
        if open {
            let pick = std::rc::Rc::new(pick);
            let (list, rows) = select_list(id, &labels, selected, 300., p);
            let mut list = list.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            for (row, (value, _)) in rows.into_iter().zip(options.iter().cloned()) {
                let pick = pick.clone();
                list = list.child(row.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_select = None;
                    pick(this, value.clone(), window, cx);
                })));
            }
            wrap = wrap.child(div().absolute().top(px(32.)).left_0().child(gpui_kit::deferred(gpui_kit::anchored().child(list)).with_priority(4)));
        }
        wrap.into_any_element()
    }
}
