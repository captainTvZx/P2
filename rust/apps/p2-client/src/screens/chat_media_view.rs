//! Medya ekleri ve bağlantı kartlarının görünümü (L ajanı) — MessageRichText.tsx `YoutubeCard`,
//! `VideoPreview` (`.message-video.p2-video`), `AudioPreview` (`.message-audio` + Chromium `<audio controls>`),
//! `.message-pdf-preview` (pdf.js küçük resmi). Ölçüler Electron'da hesaplanan değerler.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Bounds, Hsla, ImageSource, MouseButton, MouseDownEvent, ObjectFit, Pixels, SharedString, canvas, div, img, px};
use p2_ui::P2Text as _;
use p2_ui::tokens::{FONT_FALLBACK, Palette, Shadow, css_rgba, fs, shadows};

use super::extras::{FileRef, ImageCache, Slot, format_size};
use super::media::{Player, clock};
use super::view::{RowAct, RowCb};

fn emit(cb: &RowCb, act: RowAct) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut gpui_kit::App) + 'static {
    let cb = cb.clone();
    move |_, window, cx| {
        cx.stop_propagation();
        cb(act.clone(), window, cx)
    }
}

fn is_light(p: &Palette) -> bool {
    p.bg_0.l > 0.5
}

/// `.youtube-card`: genişlik min(560, %100), 16:9, 1px kenar, 11px yarıçap, `bg-1`; ortada 62px
/// oynat dairesi (`rgba(7,11,20,.72)`, üzerine gelince `danger`). Tıklayınca Electron gömülü oynatıcı
/// açar; GPUI'de web görünümü yok → video tarayıcıda açılır (bilinen fark).
pub fn youtube_card(id: &str, yt: &str, images: &ImageCache, p: &Palette, cb: &RowCb) -> AnyElement {
    let gid = SharedString::from(format!("yt-{id}-{yt}"));
    let thumb = match images.get(&format!("yt:{yt}")) {
        // object-fit: cover (558×313 iç alan): ölçekle, ortala, taşanı kırp.
        Some(Slot::Ready(image, w, h)) => {
            let (iw, ih) = (558f32, 313f32);
            let k = (iw / *w as f32).max(ih / *h as f32);
            let (dw, dh) = (*w as f32 * k, *h as f32 * k);
            Some(img(ImageSource::Render(image.clone())).absolute().left(px((iw - dw) / 2.)).top(px((ih - dh) / 2.)).w(px(dw)).h(px(dh)))
        }
        _ => None,
    };
    let play = |bg: &str| {
        // svg 62×62, padding-left 4 → içerik 58×62; viewBox 24 "meet" → ölçek 58/24, dikey 2px kaydırma.
        let k = 58. / 24.;
        let pt = |x: f32, y: f32| format!("{:.3},{:.3}", 4. + x * k, 2. + y * k);
        p2_ui::icons::svg_image(format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="62" height="62" viewBox="0 0 62 62"><circle cx="31" cy="31" r="31" fill="{bg}"/><path d="M{} L{} L{} Z" fill="#fff"/></svg>"##,
            pt(8., 5.5),
            pt(8., 18.5),
            pt(19., 12.)
        ))
        .size(px(62.))
    };
    let danger = p2_ui::icons::svg_color(p.danger);
    let danger = format!("rgba({},{})", danger.0.trim_start_matches("rgb(").trim_end_matches(')'), danger.1);
    div()
        .id(gid.clone())
        .group(gid.clone())
        .relative()
        .mt(px(9.))
        .w(px(560.))
        .max_w_full()
        .h(px(315.))
        .overflow_hidden()
        .border_1()
        .border_color(p.border)
        .rounded(px(11.))
        .bg(p.bg_1)
        .cursor_pointer()
        .children(thumb)
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(div().size(px(62.)).rounded_full().shadow(shadows(&[Shadow::new(0., 6., 20., 0., css_rgba(0, 0, 0, 0.4))])).child(play("rgba(7,11,20,0.72)")).group_hover(gid.clone(), |s| s.invisible())),
        )
        .child(div().absolute().inset_0().flex().items_center().justify_center().invisible().group_hover(gid, |s| s.visible()).child(play(&danger)))
        .tooltip(super::chat_header::tip("Videoyu oynat"))
        .on_click(emit(cb, RowAct::Youtube(yt.to_string())))
        .into_any_element()
}

/// "İndir" (bağlantı renkli, şeffaf düğme).
fn dl(id: String, f: &FileRef, h: f32, pad: (f32, f32), p: &Palette, cb: &RowCb) -> gpui_kit::Stateful<gpui_kit::Div> {
    let soft = p.accent_soft;
    div()
        .id(SharedString::from(id))
        .flex_none()
        .h(px(h))
        .px(px(pad.1))
        .py(px(pad.0))
        .flex()
        .items_center()
        .rounded(px(if h > 20. { 6. } else { 5. }))
        .text_size(fs(9.))
        .text_color(p.link)
        .cursor_pointer()
        .hover(move |s| s.bg(soft))
        .child("İndir")
        .on_click(emit(cb, RowAct::Download(f.clone())))
}

/// `.message-pdf-preview`: genişlik min(360, %100), canvas (pdf.js ölçek .55) %100 genişlik, en çok 310
/// yükseklik, `object-fit: contain` (`bg-3`); altlık 7/9 boşluk, ad + boyut, "İndir".
pub fn pdf_preview(key: &str, f: &FileRef, images: &ImageCache, p: &Palette, cb: &RowCb) -> AnyElement {
    let inner = 358.;
    let canvas_el: AnyElement = match images.get(&format!("pdf:{}", f.path())) {
        Some(Slot::Ready(image, w, h)) => {
            let ch = (inner * *h as f32 / *w as f32).min(310.);
            let scale = (inner / *w as f32).min(ch / *h as f32);
            let (dw, dh) = (*w as f32 * scale, *h as f32 * scale);
            div().w_full().h(px(ch)).flex().items_center().justify_center().bg(p.bg_3).child(img(ImageSource::Render(image.clone())).w(px(dw)).h(px(dh))).into_any_element()
        }
        Some(Slot::Failed) => div().w_full().h(px(150.)).flex().items_center().justify_center().text_size(fs(16.)).css_weight(800.).text_color(p.danger_text).child("PDF").into_any_element(),
        // Çizilene kadar boş canvas (300×150 → %100 genişlik).
        _ => div().w_full().h(px(179.)).bg(p.bg_3).into_any_element(),
    };
    div()
        .id(SharedString::from(format!("pdf-{key}")))
        .w(px(360.))
        .max_w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_1()
        .border_color(p.border)
        .rounded(px(11.))
        .bg(p.bg_2)
        .cursor_pointer()
        .on_click(emit(cb, RowAct::View(f.clone())))
        .child(canvas_el)
        .child(
            div()
                .w_full()
                .px(px(9.))
                .py(px(7.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .bg(p.bg_2)
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().truncate().text_size(fs(10.)).line_height(px(10.)).css_weight(700.).text_color(p.text).child(f.name.clone()))
                        .child(div().text_size(fs(8.)).line_height(px(9.)).text_color(p.text_3).child(format_size(f.size))),
                )
                .child(dl(format!("pdfdl-{key}"), f, 25., (0., 6.), p, cb)),
        )
        .into_any_element()
}

/// Chromium aralık girdisi (`accent-color`): 4px iz, dolu kısım vurgu, 16px vurgu topuz.
/// Tıklama/sürükleme oranı `on` ile bildirilir.
pub fn range(id: String, value: f32, accent: Hsla, track: Hsla, on: Rc<dyn Fn(f32, &mut gpui_kit::Window, &mut gpui_kit::App)>) -> gpui_kit::Stateful<gpui_kit::Div> {
    let bounds: Rc<Cell<Bounds<Pixels>>> = Rc::default();
    let store = bounds.clone();
    let v = value.clamp(0., 1.);
    let (down, moved) = (on.clone(), on);
    let (b1, b2) = (bounds.clone(), bounds);
    let frac = move |b: &Rc<Cell<Bounds<Pixels>>>, x: Pixels| {
        let b = b.get();
        let w = f32::from(b.size.width) - 16.;
        if w <= 0. { 0. } else { ((f32::from(x - b.origin.x) - 8.) / w).clamp(0., 1.) }
    };
    div()
        .id(SharedString::from(id))
        .relative()
        .h(px(16.))
        .min_w(px(0.))
        .flex()
        .items_center()
        .cursor_pointer()
        .child(canvas(move |b, _, _| store.set(b), |_, _, _, _| {}).absolute().inset_0())
        .child(
            div()
                .w_full()
                .h(px(4.))
                .rounded(px(2.))
                .bg(track)
                .overflow_hidden()
                .child(div().h_full().w(gpui_kit::relative(v)).bg(accent)),
        )
        .child(div().absolute().top_0().size(px(16.)).rounded_full().bg(accent).with_left_frac(v))
        .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            down(frac(&b1, e.position.x), window, cx)
        })
        .on_mouse_move(move |e: &gpui_kit::MouseMoveEvent, window, cx| {
            if e.pressed_button == Some(MouseButton::Left) {
                moved(frac(&b2, e.position.x), window, cx)
            }
        })
}

/// Topuzu oranla konumlar: sol = v × (genişlik − 16).
trait LeftFrac {
    fn with_left_frac(self, v: f32) -> Self;
}
impl LeftFrac for gpui_kit::Div {
    fn with_left_frac(self, v: f32) -> Self {
        // relative(v) genişliğin v'si; topuz genişliği kadar geri kaydır: left = v·W − v·16.
        self.left(gpui_kit::relative(v)).ml(px(-16. * v))
    }
}

/// `.message-video.p2-video` (preload none: 518×150 siyah alan); oynarken kare en-boy oranıyla
/// (en çok 360 yükseklik) çizilir. Denetimler: 30 auto 1fr 30 70 30 ızgarası, `bg-1`, beyaz.
pub fn video_preview(key: &str, f: &FileRef, player: Option<&Player>, p: &Palette, cb: &RowCb) -> AnyElement {
    let (playing, pos, dur, vol, frame, dims) = match player {
        Some(pl) => {
            let s = pl.state.lock().unwrap();
            (s.playing || s.loading, s.position, s.duration, s.volume, pl.image.as_ref().map(|i| i.1.clone()), s.frame.as_ref().map(|f| (f.1, f.2)))
        }
        None => (false, 0., 0., 1., None, None),
    };
    let width = 518.;
    let vh = dims.map(|(w, h)| (width * h as f32 / w.max(1) as f32).min(360.)).unwrap_or(150.);
    let screen: AnyElement = match (frame, dims) {
        (Some(image), Some((w, h))) => {
            let scale = (width / w as f32).min(vh / h as f32);
            div().w_full().h(px(vh)).bg(gpui_kit::black()).flex().items_center().justify_center().child(img(ImageSource::Render(image)).w(px(w as f32 * scale)).h(px(h as f32 * scale))).into_any_element()
        }
        _ => div().w_full().h(px(vh)).bg(gpui_kit::black()).into_any_element(),
    };
    let (soft2, white) = (p.accent_soft_2, gpui_kit::white());
    let btn = |id: String, glyph: &'static str, act: RowAct| {
        div()
            .id(SharedString::from(id))
            .flex_none()
            .w(px(28.))
            .h(px(27.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .font_family(FONT_FALLBACK)
            .text_size(fs(16.))
            .text_color(white)
            .cursor_pointer()
            .hover(move |s| s.bg(soft2))
            .child(glyph)
            .on_click(emit(cb, act))
    };
    let (accent, track) = (p.accent_hover, if is_light(p) { css_rgba(239, 239, 239, 1.) } else { css_rgba(59, 59, 59, 1.) });
    let (sc, vc) = (cb.clone(), cb.clone());
    let (sf, vf) = (f.clone(), f.clone());
    let seek: Rc<dyn Fn(f32, &mut gpui_kit::Window, &mut gpui_kit::App)> = Rc::new(move |v, w, cx| sc(RowAct::Seek(sf.clone(), v), w, cx));
    let volume: Rc<dyn Fn(f32, &mut gpui_kit::Window, &mut gpui_kit::App)> = Rc::new(move |v, w, cx| vc(RowAct::Volume(vf.clone(), v), w, cx));
    let fraction = if dur > 0. { (pos / dur) as f32 } else { 0. };
    div()
        .id(SharedString::from(format!("vid-{key}")))
        .relative()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_1()
        .border_color(p.border)
        .rounded(px(11.))
        .bg(p.bg_0)
        .child(div().id(SharedString::from(format!("vids-{key}"))).cursor_pointer().child(screen).on_click(emit(cb, RowAct::Play(f.clone()))))
        .child(
            div()
                .h(px(41.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(7.))
                .bg(p.bg_1)
                .text_color(white)
                .child(div().w(px(30.)).child(btn(format!("vp-{key}"), if playing { "Ⅱ" } else { "▶" }, RowAct::Play(f.clone()))))
                .child(div().flex_none().text_size(fs(9.)).whitespace_nowrap().child(format!("{} / {}", clock(pos), clock(dur))))
                .child(div().flex_1().min_w(px(70.)).child(range(format!("vr-{key}"), fraction, accent, track, seek)))
                .child(div().w(px(30.)).child(btn(format!("vm-{key}"), if vol > 0. { "🔊" } else { "🔇" }, RowAct::Mute(f.clone()))))
                .child(div().w(px(70.)).child(range(format!("vv-{key}"), vol, accent, track, volume)))
                .child(div().w(px(30.)).child(btn(format!("vf-{key}"), "⛶", RowAct::Fullscreen(f.clone())))),
        )
        .child(
            div()
                .px(px(9.))
                .py(px(6.))
                .flex()
                .items_center()
                .justify_between()
                .text_size(fs(9.))
                .text_color(p.text_3)
                .child(format!("{} · {}", f.name, format_size(f.size)))
                .child(dl(format!("vdl-{key}"), f, 15., (3., 6.), p, cb)),
        )
        .into_any_element()
}

/// `.message-audio`: ad/boyut | Chromium `<audio controls>` (280×30 hap) | "İndir".
pub fn audio_preview(key: &str, f: &FileRef, player: Option<&Player>, p: &Palette, cb: &RowCb) -> AnyElement {
    let (playing, pos, dur, vol) = match player {
        Some(pl) => {
            let s = pl.state.lock().unwrap();
            (s.playing || s.loading, s.position, s.duration, s.volume)
        }
        None => (false, 0., 0., 1.),
    };
    // Chromium medya denetimi: koyu şemada rgb(55,56,58) hap, beyaz simge/metin, rgb(95,96,97) iz.
    let light = is_light(p);
    let (pill, fg, track, done) = if light {
        (css_rgba(241, 243, 244, 1.), css_rgba(0, 0, 0, 0.87), css_rgba(0, 0, 0, 0.12), css_rgba(0, 0, 0, 0.87))
    } else {
        (css_rgba(55, 56, 58, 1.), gpui_kit::white(), css_rgba(95, 96, 97, 1.), gpui_kit::white())
    };
    let (fgc, _) = p2_ui::icons::svg_color(fg);
    let glyph = |svg: String| p2_ui::icons::svg_image(svg);
    let play_icon = if playing {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20"><rect x="5" y="4" width="3.5" height="12" fill="{fgc}"/><rect x="11.5" y="4" width="3.5" height="12" fill="{fgc}"/></svg>"#)
    } else {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20"><path d="M6 4 L16 10 L6 16 Z" fill="{fgc}"/></svg>"#)
    };
    let volume_icon = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" opacity="{}"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3A4.5 4.5 0 0 0 14 8v8a4.47 4.47 0 0 0 2.5-4zM14 3.23v2.06a7 7 0 0 1 0 13.42v2.06A9 9 0 0 0 14 3.23z" fill="{fgc}"/></svg>"#,
        if vol > 0. { 0.45 } else { 0.3 }
    );
    let kebab = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 20 20"><circle cx="10" cy="4.5" r="1.6" fill="{fgc}"/><circle cx="10" cy="10" r="1.6" fill="{fgc}"/><circle cx="10" cy="15.5" r="1.6" fill="{fgc}"/></svg>"#);
    let (sc, sf) = (cb.clone(), f.clone());
    let seek: Rc<dyn Fn(f32, &mut gpui_kit::Window, &mut gpui_kit::App)> = Rc::new(move |v, w, cx| sc(RowAct::Seek(sf.clone(), v), w, cx));
    let fraction = if dur > 0. { (pos / dur) as f32 } else { 0. };
    let control = div()
        .w(px(280.))
        .h(px(30.))
        .flex()
        .items_center()
        .rounded(px(15.))
        .bg(pill)
        .text_color(fg)
        .child(div().id(SharedString::from(format!("ap-{key}"))).ml(px(16.)).size(px(20.)).cursor_pointer().child(glyph(play_icon).size(px(20.))).on_click(emit(cb, RowAct::Play(f.clone()))))
        .child(div().ml(px(11.)).flex_none().text_size(px(14.)).child(format!("{} / {}", clock(pos), clock(dur))))
        .child(div().ml(px(16.)).w(px(61.)).child(range(format!("ar-{key}"), fraction, done, track, seek).h(px(12.))))
        .child(div().id(SharedString::from(format!("am-{key}"))).ml(px(21.)).size(px(20.)).cursor_pointer().child(glyph(volume_icon).size(px(20.))).on_click(emit(cb, RowAct::Mute(f.clone()))))
        .child(div().ml(px(13.)).size(px(20.)).child(glyph(kebab).size(px(20.))));
    div()
        .id(SharedString::from(format!("aud-{key}")))
        .w_full()
        .h(px(50.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .border_1()
        .border_color(p.border)
        .rounded(px(11.))
        .bg(p.bg_3)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(div().truncate().text_size(fs(10.)).line_height(px(10.)).css_weight(700.).text_color(p.text).child(f.name.clone()))
                .child(div().text_size(fs(8.)).line_height(px(9.)).text_color(p.text_3).child(format_size(f.size))),
        )
        .child(control)
        .child(dl(format!("adl-{key}"), f, 11., (1., 6.), p, cb))
        .into_any_element()
}

// ---------------------------------------------------------------- oynatıcı denetimi (sohbet ve iş parçacığı ortak)

use std::sync::Arc;

use gpui_kit::{Context, Window};
use p2_client_core::Api;

use super::ChatPane;
use super::extras::{Kind, preview_kind};
use super::media::Players;

/// Oynatıcı eylemleri. `get` görünümün oynatıcı tablosunu verir (yenileme döngüsü için).
pub fn media_act<V: 'static>(players: &mut Players, api: &Arc<Api>, act: RowAct, get: fn(&mut V) -> &mut Players, window: &mut Window, cx: &mut Context<V>) {
    match act {
        RowAct::Youtube(id) => cx.open_url(&format!("https://www.youtube.com/watch?v={id}")),
        RowAct::Play(f) => {
            let video = preview_kind(&f.name, &f.mime) == Kind::Video;
            let player = players.map.entry(f.path()).or_insert_with(|| Player::new(video));
            let (api, path) = (api.clone(), f.path());
            let ext = f.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_else(|| "bin".into());
            crate::trace(&format!("medya: oynat/duraklat {}", f.name));
            player.toggle(move || api.get_bytes(&path).ok(), ext);
            ensure_tick(players, get, window, cx);
        }
        RowAct::Seek(f, v) => {
            let video = preview_kind(&f.name, &f.mime) == Kind::Video;
            let player = players.map.entry(f.path()).or_insert_with(|| Player::new(video));
            let dur = player.state.lock().map(|s| s.duration).unwrap_or(0.);
            player.seek(dur * v as f64);
            cx.notify();
        }
        RowAct::Volume(f, v) => {
            let video = preview_kind(&f.name, &f.mime) == Kind::Video;
            players.map.entry(f.path()).or_insert_with(|| Player::new(video)).set_volume(v);
            cx.notify();
        }
        RowAct::Mute(f) => {
            let video = preview_kind(&f.name, &f.mime) == Kind::Video;
            let player = players.map.entry(f.path()).or_insert_with(|| Player::new(video));
            let v = player.state.lock().map(|s| s.volume).unwrap_or(1.);
            player.set_volume(if v > 0. { 0. } else { 1. });
            cx.notify();
        }
        RowAct::Fullscreen(f) => {
            if let Some(p) = players.map.get_mut(&f.path()) {
                p.fullscreen = !p.fullscreen;
            } else {
                let mut p = Player::new(true);
                p.fullscreen = true;
                players.map.insert(f.path(), p);
            }
            cx.notify();
        }
        _ => {}
    }
}

/// Oynayan varken ~30 fps yenile: yeni kareleri GPUI görüntüsüne çevirir (eskisinin dokusu bırakılır).
fn ensure_tick<V: 'static>(players: &mut Players, get: fn(&mut V) -> &mut Players, window: &mut Window, cx: &mut Context<V>) {
    if players.ticking {
        return;
    }
    players.ticking = true;
    cx.spawn_in(window, async move |this, cx| loop {
        cx.background_executor().timer(std::time::Duration::from_millis(33)).await;
        let go = this.update(cx, |view, cx| {
            let players = get(view);
            let mut any = false;
            let mut dropped = Vec::new();
            for p in players.map.values_mut() {
                let mut s = p.state.lock().unwrap();
                any |= s.playing || s.loading;
                if p.image.as_ref().map(|i| i.0) != Some(s.frame_epoch) {
                    if let Some((bgra, w, h)) = s.frame.take() {
                        if let Some(buf) = image::RgbaImage::from_raw(w, h, bgra.clone()) {
                            let img = Arc::new(gpui_kit::RenderImage::new(vec![image::Frame::new(buf)]));
                            if let Some((_, old)) = p.image.replace((s.frame_epoch, img)) {
                                dropped.push(old);
                            }
                        }
                        s.frame = Some((bgra, w, h));
                    }
                }
            }
            if !any {
                players.ticking = false;
            }
            for old in dropped {
                cx.drop_image(old, None);
            }
            cx.notify();
            any
        });
        if !matches!(go, Ok(true)) {
            break;
        }
    })
    .detach();
}

impl ChatPane {
    pub(super) fn media_act(&mut self, act: RowAct, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api_arc();
        let mut players = std::mem::take(&mut self.players);
        media_act(&mut players, &api, act, chat_players, window, cx);
        // Döngü bayrağı ve oynatıcılar geri.
        let ticking = players.ticking || self.players.ticking;
        self.players.map.extend(players.map);
        self.players.ticking = ticking;
        cx.notify();
    }

    /// pdf.js `PdfThumbnail`: ilk sayfa, ölçek .55.
    pub(super) fn load_pdf_thumb(&mut self, f: &FileRef, window: &mut Window, cx: &mut Context<Self>) {
        let key = format!("pdf:{}", f.path());
        if self.images_mut().contains_key(&key) {
            return;
        }
        self.images_mut().insert(key.clone(), Slot::Loading);
        let (api, path) = (self.api_arc(), f.path());
        crate::task::run(
            window,
            cx,
            move || {
                let bytes = api.get_bytes(&path).ok()?;
                let pages = super::media::render_pdf(&bytes, 1, 0.55).ok()?;
                pages.pages.into_iter().next().map(|(i, w, h, _, _)| Slot::Ready(i, w, h))
            },
            move |this, r, _, cx| {
                crate::trace(&format!("medya: pdf küçük resmi {}", if r.is_some() { "hazır" } else { "yok" }));
                this.images_mut().insert(key, r.unwrap_or(Slot::Failed));
                cx.notify();
            },
        );
    }

    /// `YoutubeCard` küçük resmi (i.ytimg.com — Electron'un `<img>` isteğiyle aynı).
    pub(super) fn load_youtube_thumb(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let key = format!("yt:{id}");
        if self.images_mut().contains_key(&key) {
            return;
        }
        self.images_mut().insert(key.clone(), Slot::Loading);
        let url = format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg");
        crate::task::run(window, cx, move || super::media::fetch_remote_image(&url), move |this, r, _, cx| {
            this.images_mut().insert(key, r.map(|(i, w, h)| Slot::Ready(i, w, h)).unwrap_or(Slot::Failed));
            cx.notify();
        });
    }
}

impl ChatPane {
    /// AttachmentViewer gövdeleri: PDF sayfaları, ofis belgesi metni, arşiv listesi, hex dökümü.
    pub(super) fn load_viewer_doc(&mut self, f: &FileRef, window: &mut Window, cx: &mut Context<Self>) {
        let kind = preview_kind(&f.name, &f.mime);
        if !matches!(kind, Kind::Pdf | Kind::Docx | Kind::Sheet | Kind::Slides | Kind::Epub | Kind::Archive | Kind::Binary) {
            self.viewer_doc = None;
            return;
        }
        self.viewer_doc = Some(super::ViewerDoc::Loading);
        let (api, path, name) = (self.api_arc(), f.path(), f.name.clone());
        crate::task::run(
            window,
            cx,
            move || {
                use super::ViewerDoc;
                let Ok(bytes) = api.get_bytes(&path) else {
                    return ViewerDoc::Error(
                        match kind {
                            Kind::Pdf => "PDF açılamadı.",
                            Kind::Archive => "Arşiv listesi okunamadı veya arşiv şifreli.",
                            Kind::Binary => "İkili önizleme alınamadı.",
                            _ => "Belge önizlemesi hazırlanamadı.",
                        }
                        .into(),
                    );
                };
                match kind {
                    Kind::Pdf => super::media::render_pdf(&bytes, 200, 1.).map(ViewerDoc::Pdf).unwrap_or_else(|_| ViewerDoc::Error("PDF açılamadı.".into())),
                    Kind::Archive => match super::media::zip_entries(&bytes).filter(|_| name.to_ascii_lowercase().ends_with(".zip")) {
                        Some(list) => ViewerDoc::Archive(list.into_iter().map(|e| (e.name, e.size)).collect()),
                        None => ViewerDoc::Error("Arşiv listesi okunamadı veya arşiv şifreli.".into()),
                    },
                    Kind::Binary => ViewerDoc::Hex(super::media::hex_dump(&bytes[..bytes.len().min(64 * 1024)])),
                    _ => super::media::office_sections(&bytes, &name).map(ViewerDoc::Office).unwrap_or_else(ViewerDoc::Error),
                }
            },
            |this, doc, _, cx| {
                this.viewer_doc = Some(doc);
                cx.notify();
            },
        );
    }

    /// Görüntüleyici gövdesi (resim/metin dışındaki türler); `None` → çağıran kendi gövdesini çizer.
    pub(super) fn viewer_doc_body(&self, f: &FileRef, zoom: f32, p: &Palette) -> Option<AnyElement> {
        use super::ViewerDoc;
        let kind = preview_kind(&f.name, &f.mime);
        let state = |t: &str| div().size_full().flex().items_center().justify_center().text_size(fs(12.)).text_color(p.text_3).child(t.to_string()).into_any_element();
        if matches!(kind, Kind::Audio | Kind::Video) {
            return Some(state("Oynatmak için mesajdaki oynatıcıyı kullan."));
        }
        if kind == Kind::Font {
            return Some(
                div()
                    .size_full()
                    .p(px(30.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(div().text_size(fs(34.)).css_weight(700.).text_color(p.text_strong).child("Aa Bb Cc Çç Ğğ 0123456789"))
                    .child(div().text_size(fs(18.)).text_color(p.text).child("P2 ile dosyalarını güvenle paylaş."))
                    .into_any_element(),
            );
        }
        let doc = self.viewer_doc.as_ref()?;
        Some(match doc {
            ViewerDoc::Loading => state(match kind {
                Kind::Pdf => "PDF hazırlanıyor…",
                Kind::Archive => "Arşiv okunuyor…",
                Kind::Binary => "Önizleme hazırlanıyor…",
                _ => "Belge hazırlanıyor…",
            }),
            ViewerDoc::Error(e) => state(e),
            ViewerDoc::Pdf(pages) => {
                let mut stage = div().id("pdf-stage").flex_1().min_h_0().overflow_y_scroll().p(px(18.)).flex().flex_col().items_center().gap(px(16.)).bg(p.bg_0);
                for (i, (image, w, h, _, _)) in pages.pages.iter().enumerate() {
                    stage = stage.child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(6.))
                            .child(img(ImageSource::Render(image.clone())).w(px(*w as f32 * zoom)).h(px(*h as f32 * zoom)))
                            .child(div().text_size(fs(9.)).text_color(p.text_3).child((i + 1).to_string())),
                    );
                }
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(44.))
                            .flex_none()
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .bg(p.bg_1)
                            .child(div().text_size(fs(11.)).css_weight(700.).text_color(p.text).child(format!("{} sayfa", pages.count)))
                            .child(div().flex_1())
                            .child(div().text_size(fs(11.)).text_color(p.text_2).child(format!("{}%", (zoom * 100.).round()))),
                    )
                    .child(stage)
                    .into_any_element()
            }
            ViewerDoc::Office(sections) => {
                let mut col = div().id("viewer-office").size_full().overflow_y_scroll().py(px(24.)).px(px(30.)).flex().flex_col().gap(px(18.)).bg(p.bg_0).text_size(fs(12.)).line_height(gpui_kit::relative(1.6)).text_color(p.text);
                for s in sections {
                    let mut sec = div().flex().flex_col().gap(px(8.));
                    if let Some(t) = &s.title {
                        sec = sec.child(div().text_size(fs(14.)).css_weight(700.).text_color(p.text_strong).child(t.clone()));
                    }
                    for para in &s.paragraphs {
                        sec = sec.child(div().child(para.clone()));
                    }
                    for row in &s.rows {
                        let mut r = div().flex().border_b_1().border_color(p.border_subtle);
                        for cell in row {
                            r = r.child(div().w(px(120.)).flex_none().px(px(6.)).py(px(3.)).truncate().child(cell.clone()));
                        }
                        sec = sec.child(r);
                    }
                    col = col.child(sec);
                }
                col.into_any_element()
            }
            ViewerDoc::Archive(items) => {
                let mut list = div().id("viewer-archive").flex_1().min_h_0().overflow_y_scroll().px(px(18.)).flex().flex_col();
                for (name, size) in items {
                    list = list.child(
                        div()
                            .py(px(7.))
                            .flex()
                            .justify_between()
                            .gap(px(12.))
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .child(div().min_w_0().truncate().text_size(fs(11.)).text_color(p.text).child(name.clone()))
                            .child(div().text_size(fs(9.)).text_color(p.text_3).child(format_size(*size))),
                    );
                }
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(div().px(px(18.)).py(px(14.)).flex().items_baseline().gap(px(10.)).child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text_strong).child(format!("{} öğe", items.len()))).child(div().text_size(fs(9.)).text_color(p.text_3).child("Dosyalar çıkarılmadan listeleniyor")))
                    .child(list)
                    .into_any_element()
            }
            ViewerDoc::Hex(text) => div()
                .size_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(18.))
                        .py(px(14.))
                        .flex()
                        .items_baseline()
                        .gap(px(10.))
                        .child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text_strong).child(if f.mime.is_empty() { "application/octet-stream".to_string() } else { f.mime.clone() }))
                        .child(div().text_size(fs(9.)).text_color(p.text_3).child(format!("İlk {} güvenli hex görünümü", format_size(f.size.min(64 * 1024))))),
                )
                .child(div().id("viewer-hex").flex_1().min_h_0().overflow_y_scroll().px(px(18.)).font_family(p2_ui::tokens::FONT_MONO).text_size(px(11.)).line_height(px(17.)).text_color(p.text).child(text.clone()))
                .into_any_element(),
        })
    }
}

fn chat_players(c: &mut ChatPane) -> &mut Players {
    &mut c.players
}
