//! Proje ekran görüntüsü galerisi (ProjectScreenshotsPanel.tsx + project-screenshots.css): araç
//! çubuğu (adet + Yenile), 16:9 ızgara (minmax 190px), büyütme (lightbox: başlık düzenleme, paylaş,
//! sil), paylaşım penceresi (kanala mesaj = başlık + ek işareti) ve silme onayı.

use std::collections::HashMap;
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, FontWeight, ImageSource, MouseButton, ObjectFit, RenderImage, SharedString, Subscription, Window, div, img, point, px};
use p2_client_core::{Api, ChannelType, ChatTarget, ProjectScreenshot, UploadedFile};
use p2_ui::P2Text as _;
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, empty, modal, modal_foot};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, css_rgba, font_size, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::ui::{fmt_bytes, ico, lib_avatar, note_time_ago};
use crate::task;
use p2_i18n::{t, tv};

const SHARE: &str = r#"<path d="M4 12v7a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-7"/><path d="M12 15V4M7.5 8.5 12 4l4.5 4.5"/>"#;
const TRASH: &str = r#"<path d="M5 7h14M10 7V4h4v3M8 7l1 13h6l1-13"/>"#;

struct Share {
    shot: ProjectScreenshot,
    caption: Entity<InputState>,
    filter: Entity<InputState>,
    channels: Option<Vec<(String, String, ChannelType, String)>>,
    target: Option<(String, String)>,
    busy: bool,
    error: Option<String>,
    _subs: Vec<Subscription>,
}

pub struct ShotsPanel {
    api: Arc<Api>,
    org: String,
    project: String,
    me: String,
    manage: bool,
    items: Vec<ProjectScreenshot>,
    loading: bool,
    thumbs: HashMap<String, Arc<RenderImage>>,
    full: HashMap<String, Arc<RenderImage>>,
    open: Option<String>,
    caption: Entity<InputState>,
    caption_saving: bool,
    share: Option<Share>,
    confirm: Option<String>,
    width: f32,
    _subs: Vec<Subscription>,
}

fn decode(bytes: &[u8], max: Option<(u32, u32)>) -> Option<Arc<RenderImage>> {
    let image = image::load_from_memory(bytes).ok()?;
    let image = match max {
        Some((w, h)) => image.thumbnail(w, h),
        None => image,
    };
    let mut bgra = image.to_rgba8();
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
}

fn dims(s: &ProjectScreenshot) -> String {
    format!("{}×{}", s.width, s.height)
}

impl ShotsPanel {
    pub fn new(api: Arc<Api>, org: String, project: String, me: String, manage: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let caption = cx.new(|cx| InputState::new(window, cx).placeholder(t("shots.captionPlaceholder")));
        let subs = vec![cx.subscribe_in(&caption, window, |this, _, e: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = e {
                this.save_caption(window, cx);
            }
            cx.notify();
        })];
        let mut panel = Self { api, org, project, me, manage, items: Vec::new(), loading: true, thumbs: HashMap::new(), full: HashMap::new(), open: None, caption, caption_saving: false, share: None, confirm: None, width: 516., _subs: subs };
        panel.reload(window, cx);
        panel
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.screenshots(&org, &project), |this, result, window, cx| {
            this.loading = false;
            match result {
                Ok(items) => {
                    crate::trace(&format!("kütüphane: {} ekran görüntüsü", items.len()));
                    this.items = items;
                    this.load_thumbs(window, cx);
                }
                Err(e) => crate::toasts::error(if e.message.is_empty() { t("shots.loadFailed") } else { e.message }),
            }
            cx.notify();
        });
    }

    fn load_thumbs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for shot in self.items.iter().take(60) {
            if self.thumbs.contains_key(&shot.id) {
                continue;
            }
            let (api, id, path) = (self.api.clone(), shot.id.clone(), shot.upload_path.clone());
            task::run(window, cx, move || api.get_bytes(&path).ok().and_then(|b| decode(&b, Some((480, 270)))), move |this, image, _, cx| {
                if let Some(image) = image {
                    this.thumbs.insert(id, image);
                    cx.notify();
                }
            });
        }
    }

    fn can_edit(&self, s: &ProjectScreenshot) -> bool {
        s.user.id == self.me || self.manage
    }

    fn open_shot(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shot) = self.items.iter().find(|s| s.id == id).cloned() else { return };
        let caption = shot.caption.clone().unwrap_or_default();
        self.caption.update(cx, |s, cx| s.set_value(caption, window, cx));
        self.open = Some(id.clone());
        if !self.full.contains_key(&id) {
            let api = self.api.clone();
            task::run(window, cx, move || api.get_bytes(&shot.upload_path).ok().and_then(|b| decode(&b, None)), move |this, image, _, cx| {
                if let Some(image) = image {
                    this.full.insert(id, image);
                    cx.notify();
                }
            });
        }
        cx.notify();
    }

    fn save_caption(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.open.clone() else { return };
        let caption = self.caption.read(cx).value().trim().to_string();
        if self.items.iter().find(|s| s.id == id).is_some_and(|s| s.caption.clone().unwrap_or_default() == caption) {
            return;
        }
        self.caption_saving = true;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.update_screenshot(&org, &project, &id, &caption), |this, result, _, cx| {
            this.caption_saving = false;
            match result {
                Ok(saved) => {
                    if let Some(s) = this.items.iter_mut().find(|s| s.id == saved.id) {
                        s.caption = saved.caption;
                    }
                }
                Err(e) => crate::toasts::error(if e.message.is_empty() { t("shots.saveFailed") } else { e.message }),
            }
            cx.notify();
        });
    }

    fn remove(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.delete_screenshot(&org, &project, &id).map(|_| id), |this, result, _, cx| {
            match result {
                Ok(id) => {
                    this.items.retain(|s| s.id != id);
                    crate::toasts::ok(t("shots.deleted"));
                    crate::trace("kütüphane: ekran görüntüsü silindi");
                }
                Err(e) => crate::toasts::error(if e.message.is_empty() { t("shots.deleteFailed") } else { e.message }),
            }
            cx.notify();
        });
    }

    fn open_share(&mut self, shot: ProjectScreenshot, window: &mut Window, cx: &mut Context<Self>) {
        let caption = cx.new(|cx| InputState::new(window, cx).placeholder(t("shots.share.captionPlaceholder")));
        let c = shot.caption.clone().unwrap_or_default();
        caption.update(cx, |s, cx| {
            s.set_value(c, window, cx);
            s.focus(window, cx);
        });
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(t("shots.share.searchChannel")));
        let subs = vec![cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify())];
        self.share = Some(Share { shot, caption, filter, channels: None, target: None, busy: false, error: None, _subs: subs });
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.channels(&org, &project), |this, result, _, cx| {
            if let Some(s) = this.share.as_mut() {
                match result {
                    Ok(cats) => {
                        let list: Vec<_> = cats.iter().flat_map(|c| c.channels.iter().filter(|ch| matches!(ch.kind, ChannelType::Text | ChannelType::Announcement)).map(move |ch| (ch.id.clone(), ch.name.clone(), ch.kind, c.name.clone()))).collect();
                        if s.target.is_none() {
                            s.target = list.first().map(|c| (c.0.clone(), c.1.clone()));
                        }
                        s.channels = Some(list);
                    }
                    Err(e) => {
                        s.channels = Some(Vec::new());
                        s.error = Some(if e.message.is_empty() { t("shots.share.channelsFailed") } else { e.message });
                    }
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    fn send_share(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.share.as_mut() else { return };
        let Some((channel, name)) = s.target.clone() else { return };
        s.busy = true;
        s.error = None;
        let file = UploadedFile { name: format!("ekran-{}.jpg", s.shot.id), mime: "image/jpeg".into(), size: s.shot.size.unwrap_or(0), url: s.shot.upload_path.clone() };
        let caption = s.caption.read(cx).value().trim().to_string();
        let content = [caption, p2_client_core::attachment_marker(&file)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n");
        let target = ChatTarget::Channel { org: self.org.clone(), project: self.project.clone(), channel };
        let (api, me) = (self.api.clone(), self.me.clone());
        task::run(window, cx, move || api.send_message(&target, &content, None, &me), move |this, result, _, cx| {
            match result {
                Ok(_) => {
                    this.share = None;
                    crate::toasts::ok(tv("shots.share.sent", &[("name", &name)]));
                    crate::trace(&format!("kütüphane: ekran görüntüsü #{name} kanalına paylaşıldı"));
                }
                Err(e) => {
                    if let Some(s) = this.share.as_mut() {
                        s.busy = false;
                        s.error = Some(if e.message.is_empty() { t("shots.share.sendFailed") } else { e.message });
                    }
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    fn layer(id: &'static str, priority: usize, dim: bool, window: &Window, p: &Palette, child: impl IntoElement, close: fn(&mut Self), cx: &mut Context<Self>) -> AnyElement {
        let size = window.viewport_size();
        let layer = div()
            .id(id)
            .w(size.width)
            .h(size.height)
            .flex()
            .items_center()
            .justify_center()
            .pt(px(38.))
            .px(px(24.))
            .pb(px(24.))
            .bg(if dim { css_rgba(2, 4, 10, 0.82) } else { p.overlay }).backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                close(this);
                cx.notify();
            }))
            .child(div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(child))
            .p2_enter(SharedString::from(format!("{id}-fade")), Entrance::Fade, Duration::ZERO);
        gpui_kit::deferred(gpui_kit::anchored().position_mode(gpui_kit::AnchoredPositionMode::Window).position(point(px(0.), px(0.))).child(layer)).with_priority(priority).into_any_element()
    }

    fn lightbox(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.open.clone()?;
        let shot = self.items.iter().find(|s| s.id == id)?.clone();
        let vw = f32::from(window.viewport_size().width);
        let vh = f32::from(window.viewport_size().height);
        let w = (vw - 48.).min(1200.);
        let image: AnyElement = match self.full.get(&id).or_else(|| self.thumbs.get(&id)) {
            Some(i) => img(ImageSource::Render(i.clone())).w_full().max_h(px(vh - 150.)).object_fit(ObjectFit::Contain).into_any_element(),
            None => div().h(px(240.)).into_any_element(),
        };
        let can_edit = self.can_edit(&shot);
        let dirty = self.caption.read(cx).value().trim() != shot.caption.clone().unwrap_or_default();
        let meta = format!("{} · {}{}", note_time_ago(&shot.taken_at), dims(&shot), shot.size.filter(|s| *s > 0).map(|s| format!(" · {}", fmt_bytes(s))).unwrap_or_default());
        let (s1, s2) = (shot.clone(), shot.id.clone());
        let card = div()
            .w(px(w))
            .max_h(px(vh - 48.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::XL)
            .bg(p.bg_1)
            .shadow(p.shadow_lg())
            .child(div().flex_1().min_h_0().bg(p.bg_0).flex().items_center().justify_center().child(image))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(12.))
                    .gap_y(px(8.))
                    .py(px(10.))
                    .px(px(14.))
                    .border_t_1()
                    .border_color(p.border_subtle)
                    .child(div().flex().items_center().gap(px(6.)).min_w_0().child(lib_avatar(&shot.user.id, &shot.user.display_name, shot.user.avatar_url.as_deref(), 20., 7., p)).child(div().text_size(font_size::SM).text_color(p.text_strong).font_weight(FontWeight::BOLD).whitespace_nowrap().child(shot.user.display_name.clone())).child(div().text_size(font_size::XS).text_color(p.text_3).whitespace_nowrap().child(meta)))
                    .map(|s| {
                        if can_edit {
                            s.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .flex_1()
                                    .min_w(px(200.))
                                    .child(div().flex_1().min_w_0().h(px(30.)).px(px(12.)).flex().items_center().border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_3).text_size(font_size::MD).child(Input::new(&self.caption).appearance(false)))
                                    .when(dirty, |s| s.child(button("ps-cap-save", BtnVariant::Primary, BtnSize::Sm, false, false, p).child(if self.caption_saving { t("shots.saving") } else { t("shots.save") }).on_click(cx.listener(|this, _, window, cx| this.save_caption(window, cx))))),
                            )
                        } else if let Some(c) = shot.caption.clone().filter(|c| !c.is_empty()) {
                            s.child(div().flex_1().min_w(px(200.)).text_size(font_size::SM).text_color(p.text).child(c))
                        } else {
                            s
                        }
                    })
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(button("ps-lb-share", BtnVariant::Default, BtnSize::Sm, false, false, p).child(ico(SHARE, 1.8, 15., p.text)).child(t("shots.shareBtn")).on_click(cx.listener(move |this, _, window, cx| this.open_share(s1.clone(), window, cx))))
                            .when(can_edit, |s| {
                                s.child(button("ps-lb-del", BtnVariant::Danger, BtnSize::Sm, false, false, p).child(ico(TRASH, 1.8, 15., p.danger_text)).child(t("shots.delete")).on_click(cx.listener(move |this, _, _, cx| {
                                    this.confirm = Some(s2.clone());
                                    cx.notify();
                                })))
                            })
                            .child(button("ps-lb-close", BtnVariant::Ghost, BtnSize::Sm, false, false, p).child(t("shots.close")).on_click(cx.listener(|this, _, _, cx| {
                                this.open = None;
                                cx.notify();
                            }))),
                    ),
            )
            .p2_enter("ps-lightbox", Entrance::PopElastic, Duration::ZERO);
        Some(Self::layer("ps-lightbox-layer", 6, true, window, p, card, |t| t.open = None, cx))
    }

    fn share_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.share.as_ref()?;
        let q = s.filter.read(cx).value().trim().to_lowercase();
        let mut list = div().id("ps-share-list").max_h(px(240.)).overflow_y_scroll().flex().flex_col().gap(px(2.)).p(px(4.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_2);
        match &s.channels {
            None => list = list.child(div().text_color(p.text_3).text_size(font_size::XS).child(t("shots.loading"))),
            Some(chs) => {
                let visible: Vec<_> = chs.iter().filter(|c| q.is_empty() || c.1.to_lowercase().contains(&q)).collect();
                if visible.is_empty() {
                    list = list.child(div().text_color(p.text_3).text_size(font_size::XS).child(t("shots.share.noMatch")));
                }
                for (id, name, kind, cat) in visible {
                    let active = s.target.as_ref().is_some_and(|t| &t.0 == id);
                    let (tid, tname) = (id.clone(), name.clone());
                    let hover = p.bg_hover;
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("ps-share-{id}")))
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .py(px(6.))
                            .px(px(8.))
                            .border_1()
                            .rounded(radius::XS)
                            .cursor_pointer()
                            .map(|e| if active { e.bg(p.bg_selected).border_color(p.border_accent) } else { e.border_color(gpui_kit::transparent_black()).hover(move |e| e.bg(hover)) })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.share.as_mut() {
                                    s.target = Some((tid.clone(), tname.clone()));
                                }
                                cx.notify();
                            }))
                            .child(div().size(px(18.)).flex().items_center().justify_center().text_color(p.text_3).child(p2_ui::icons::icon(if *kind == ChannelType::Announcement { "channel:announcement" } else { "channel:text" }).el().size(px(16.))))
                            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(font_size::SM).font_weight(FontWeight::SEMIBOLD).text_color(p.text_strong).child(name.clone()))
                            .child(div().text_size(font_size::XXS).text_color(p.text_4).whitespace_nowrap().child(cat.clone())),
                    );
                }
            }
        }
        let field = |label: &str, input: &Entity<InputState>| {
            div().flex().flex_col().gap(px(6.)).child(div().text_size(font_size::SM).css_weight(650.).text_color(p.text_2).child(label.to_string())).child(div().h(px(34.)).px(px(12.)).flex().items_center().border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_3).text_size(font_size::MD).child(Input::new(input).appearance(false)))
        };
        let can = s.target.is_some() && !s.busy;
        let card = modal(ModalSize::Sm, window, p)
            .child(div().pt(px(18.)).pr(px(16.)).pb(px(12.)).pl(px(20.)).flex().flex_col().child(div().text_size(font_size::XL).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(t("shots.shareBtn"))).child(div().mt(px(3.)).text_size(font_size::SM).text_color(p.text_3).child(t("shots.share.subtitle"))))
            .child(
                div()
                    .pt(px(4.))
                    .px(px(20.))
                    .pb(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(field(&t("shots.share.caption"), &s.caption))
                    .child(field(&t("shots.share.target"), &s.filter))
                    .child(list)
                    .when_some(s.error.clone(), |e, err| e.child(div().py(px(6.)).px(px(10.)).border_1().border_color(p.danger_border).rounded(radius::SM).bg(p.danger_soft).text_color(p.danger_text).text_size(font_size::XS).child(err))),
            )
            .child(
                modal_foot(p)
                    .child(button("ps-share-no", BtnVariant::Ghost, BtnSize::Md, false, false, p).child(t("shots.cancel")).on_click(cx.listener(|this, _, _, cx| {
                        this.share = None;
                        cx.notify();
                    })))
                    .child(button("ps-share-go", BtnVariant::Primary, BtnSize::Md, false, false, p).when(!can, |s| s.opacity(0.5)).child(if s.busy { t("shots.share.sending") } else { t("shots.share.send") }).when(can, |b| b.on_click(cx.listener(|this, _, window, cx| this.send_share(window, cx))))),
            )
            .p2_enter("ps-share-card", Entrance::PopElastic, Duration::ZERO);
        Some(Self::layer("ps-share-layer", 7, false, window, p, card, |t| t.share = None, cx))
    }

    fn confirm_view(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.confirm.clone()?;
        let card = modal(ModalSize::Sm, window, p)
            .child(p2_ui::controls::modal_head(t("shots.confirm.title"), Some(t("shots.confirm.detail").into()), Some(button("ps-del-x", BtnVariant::Ghost, BtnSize::Sm, true, false, p).child("×").on_click(cx.listener(|this, _, _, cx| {
                this.confirm = None;
                cx.notify();
            })).into_any_element()), p))
            .child(p2_ui::controls::modal_body())
            .child(
                modal_foot(p)
                    .child(button("ps-del-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child(t("shots.cancel")).on_click(cx.listener(|this, _, _, cx| {
                        this.confirm = None;
                        cx.notify();
                    })))
                    .child(button("ps-del-yes", BtnVariant::Danger, BtnSize::Md, false, false, p).child(t("shots.delete")).on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm = None;
                        this.remove(id.clone(), window, cx);
                    }))),
            )
            .p2_enter("ps-del-card", Entrance::PopElastic, Duration::ZERO);
        Some(Self::layer("ps-del-layer", 8, false, window, p, card, |t| t.confirm = None, cx))
    }
}

impl Render for ShotsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let p = &p;
        let toolbar = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_h(px(28.))
            .child(div().text_color(p.text_3).text_size(font_size::XS).child(if self.items.is_empty() { String::new() } else { tv("shots.count", &[("n", &self.items.len().to_string())]) }))
            .child(div().flex_1())
            .child(button("ps-reload", BtnVariant::Ghost, BtnSize::Sm, false, false, p).tooltip(|w, cx| p2_ui::controls::tooltip(t("shots.refresh"), w, cx)).child(t("shots.refresh")).on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))));
        let body: AnyElement = if self.loading {
            empty("", Some(t("shots.loadingList").into()), p).py(px(28.)).px(px(12.)).into_any_element()
        } else if self.items.is_empty() {
            empty(t("shots.empty.title"), Some(t("shots.empty.detail").into()), p).py(px(28.)).px(px(12.)).into_any_element()
        } else {
            // repeat(auto-fill, minmax(190px, 1fr)), gap 12 — sütun sayısı panel genişliğinden.
            let width = self.width.max(190.);
            let cols = (((width + 12.) / (190. + 12.)).floor() as usize).max(1);
            let card_w = (width - 12. * (cols as f32 - 1.)) / cols as f32;
            let mut grid = div().flex().flex_wrap().gap(px(12.));
            for (n, s) in self.items.iter().enumerate() {
                let thumb = self.thumbs.get(&s.id).cloned();
                let (id, share) = (s.id.clone(), s.clone());
                let (ba, sm) = (p.border_accent, p.shadow_sm());
                let mine = self.can_edit(s);
                grid = grid.child(
                    div()
                        .w(px(card_w))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .min_w_0()
                        .child(
                            div()
                                .id(SharedString::from(format!("ps-card-{}", s.id)))
                                .w(px(card_w))
                                .h(px(card_w * 9. / 16.))
                                .overflow_hidden()
                                .border_1()
                                .border_color(p.border)
                                .rounded(radius::MD)
                                .bg(p.bg_2)
                                .cursor_pointer()
                                .hover(move |e| e.border_color(ba).shadow(sm.clone()).relative().top(px(-1.)))
                                .tooltip({
                                    let tip = s.caption.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| t("shots.enlarge"));
                                    move |w, cx| p2_ui::controls::tooltip(tip.clone(), w, cx)
                                })
                                .on_click(cx.listener(move |this, _, window, cx| this.open_shot(id.clone(), window, cx)))
                                .when_some(thumb, |e, i| e.child(img(ImageSource::Render(i)).size_full().object_fit(ObjectFit::Cover))),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .min_w_0()
                                .px(px(2.))
                                .child(lib_avatar(&s.user.id, &s.user.display_name, s.user.avatar_url.as_deref(), 20., 7., p))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap(px(1.))
                                        .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(font_size::SM).font_weight(FontWeight::SEMIBOLD).text_color(p.text_strong).child(s.caption.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| dims(s))))
                                        .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(font_size::XXS).text_color(p.text_3).child(format!("{} · {}", if s.user.id == self.me { t("shots.you") } else { s.user.display_name.clone() }, note_time_ago(&s.taken_at)))),
                                )
                                .child(button(SharedString::from(format!("ps-share-btn-{}", s.id)), BtnVariant::Ghost, BtnSize::Sm, true, false, p).tooltip(|w, cx| p2_ui::controls::tooltip(t("shots.shareBtn"), w, cx)).child(ico(SHARE, 1.8, 15., p.text_2)).on_click(cx.listener(move |this, _, window, cx| this.open_share(share.clone(), window, cx))))
                                .when(mine, |e| e.child(div().size(px(6.)).flex_shrink_0().rounded_full().bg(p.accent))),
                        )
                        .p2_enter(("ps-card-in", n), Entrance::Rise, Duration::ZERO),
                );
            }
            div().id("ps-grid").overflow_y_scroll().pr(px(2.)).child(grid).into_any_element()
        };
        let vh = f32::from(window.viewport_size().height);
        let lightbox = self.lightbox(window, p, cx);
        let share = self.share_view(window, p, cx);
        let confirm = self.confirm_view(window, p, cx);
        let me = cx.entity().downgrade();
        div()
            .max_h(px((vh * 0.6).clamp(360., 720.)))
            .flex()
            .flex_col()
            .gap(px(10.))
            .text_color(p.text)
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(gpui_kit::canvas(
                        move |bounds, _, cx| {
                            let w = f32::from(bounds.size.width);
                            let _ = me.update(cx, |this, cx| {
                                if (this.width - w).abs() > 0.5 && w > 0. {
                                    this.width = w;
                                    cx.notify();
                                }
                            });
                        },
                        |_, _, _, _| {},
                    ).absolute().w_full().h(px(0.)))
                    .relative()
                    .child(body),
            )
            .children(lightbox)
            .children(share)
            .children(confirm)
    }
}
