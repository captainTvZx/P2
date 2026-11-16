//! Ayarlar penceresinin ortak denetimleri — `src/settings.css` sınıflarının birebir GPUI karşılıkları.
//! Ölçüler Electron'un hesaplanmış stillerinden (dom-styles.mjs, P2 Mavi, ölçek 0,85):
//!
//! | settings.css | Rust |
//! |---|---|
//! | `.settings-block` (+ `h4`) | [`block`] / [`last_block`] |
//! | `.settings-hint` | [`hint`] |
//! | `.settings-toggle` (anahtar satırı) | [`toggle_row`] |
//! | `.settings-slider` / `.settings-field input[type=range]` | [`slider`] (sürükleme: [`SliderRegistry`]) |
//! | `.settings-block select` | [`select`] + [`select_menu`] |
//! | `.settings-field` (üst etiket) | [`field`] |
//! | `.settings-choices` | [`choice`] |
//! | `.settings-button(.primary/.danger)` | [`button`] |
//! | `.settings-badge(.ok/.off)` | [`badge`] |
//! | `.settings-row` | [`row`] |

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Bounds, Context, Div, ElementId, Pixels, SharedString, Stateful, Window, canvas, div, px, relative};
use p2_ui::P2Text as _;
use p2_ui::form::brighten;
use p2_ui::text::normal_line_height;
use p2_ui::tokens::{KICKER_TRACKING_EM, Palette, Shadow, css_rgba, font_size, fs, radius, shadows};

/// CSS `line-height: normal` (Chromium yuvarlaması) ile yazı.
pub fn text(size: Pixels) -> Div {
    div().text_size(size).line_height(normal_line_height(size))
}

/// `.settings-block h4`: `800 fs-2xs`, aralık .18em, büyük harf, `text-accent`.
pub fn heading(title: &str, p: &Palette) -> Div {
    text(font_size::XXS).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_accent).child(upper(title))
}

/// Türkçe büyük harf (i → İ, ı → I) — CSS `text-transform: uppercase` (lang=tr).
pub fn upper(s: &str) -> String {
    s.chars().map(|c| match c {
        'i' => "İ".to_string(),
        'ı' => "I".to_string(),
        c => c.to_uppercase().to_string(),
    }).collect()
}

/// `.settings-block`: dikey ızgara (gap 9), alt boşluk 18, alt çizgi, sonraki bloğa 16px.
pub fn block(title: &str, p: &Palette) -> Div {
    div().flex().flex_col().gap(px(9.)).pb(px(18.)).mb(px(16.)).border_b_1().border_color(p.border_subtle).child(heading(title, p))
}

/// Kapsayıcının son bloğu (`:last-child`: çizgi ve alt boşluk yok).
pub fn last_block(title: &str, p: &Palette) -> Div {
    div().flex().flex_col().gap(px(9.)).pb(px(18.)).child(heading(title, p))
}

/// `.settings-hint`: 10px, satır 1.5, `text-3`.
pub fn hint(body: impl Into<SharedString>, p: &Palette) -> Div {
    div().text_size(fs(10.)).line_height(relative(1.5)).text_color(p.text_3).child(body.into())
}

/// `.settings-row`: sarmalı yatay, gap 8.
pub fn row() -> Div {
    div().flex().flex_wrap().gap(px(8.))
}

/// `.settings-toggle`: kart satırı (p 9/11, r 11, bg-3, kenarlık), solda başlık + ipucu, sağda 36×20 anahtar.
/// `epoch` değiştikçe topuz kayarak canlanır (`p2_ui::form::toggle`).
pub fn toggle_row(id: impl Into<ElementId>, label: impl Into<SharedString>, detail: Option<SharedString>, on: bool, epoch: usize, p: &Palette) -> Stateful<Div> {
    let (hover_bg, hover_border) = (p.bg_hover, p.border_strong);
    let id: ElementId = id.into();
    div()
        .id(id.clone())
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.))
        .px(px(11.))
        .py(px(9.))
        .border_1()
        .border_color(p.border)
        .rounded(radius::MD)
        .bg(p.bg_3)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).border_color(hover_border))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w_0()
                .child(text(fs(11.5)).font_weight(gpui_kit::FontWeight::SEMIBOLD).text_color(p.text).child(label.into()))
                .when_some(detail, |s, d| s.child(text(fs(9.5)).text_color(p.text_3).child(d))),
        )
        .child(p2_ui::form::toggle((id, "knob"), on, epoch, p))
}

/// `.settings-choices button`: p 10/12, r 11, bg-3, kenarlık; etkin: `border-accent`, `accent-soft`, `accent-glow`.
pub fn choice(id: impl Into<ElementId>, label: impl Into<SharedString>, detail: impl Into<SharedString>, active: bool, p: &Palette) -> Stateful<Div> {
    let hover_bg = p.bg_hover;
    div()
        .id(id)
        .w_full()
        .flex()
        .flex_col()
        .gap(px(3.))
        .px(px(12.))
        .py(px(10.))
        .border_1()
        .rounded(radius::MD)
        .cursor_pointer()
        .map(|s| {
            if active {
                s.border_color(p.border_accent).bg(p.accent_soft).shadow(shadows(&[p.accent_glow]))
            } else {
                s.border_color(p.border).bg(p.bg_3).hover(move |s| s.bg(hover_bg))
            }
        })
        .child(text(fs(11.5)).css_weight(650.).text_color(if active { p.text_strong } else { p.text }).child(label.into()))
        // satır 1.45 (11,71px): GPUI metin kutusunu tam piksele yuvarladığından 11px + 0,71px alt boşluk.
        .child(div().text_size(fs(9.5)).line_height(px(11.)).mb(px(0.71)).text_color(if active { p.text_accent } else { p.text_3 }).child(detail.into()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Default,
    Primary,
    Danger,
}

/// `.settings-button`: h 32, p 0 14, r 8, `650 11px`.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: Btn, disabled: bool, p: &Palette) -> Stateful<Div> {
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(32.))
        .px(px(14.))
        .border_1()
        .rounded(radius::SM)
        .text_size(fs(11.))
        .line_height(normal_line_height(fs(11.)))
        .css_weight(650.)
        .child(label.into());
    let base = match kind {
        Btn::Default => {
            let (hb, hbd, ht) = (p.bg_hover, p.border_strong, p.text_strong);
            base.border_color(p.border).bg(p.bg_3).text_color(p.text).when(!disabled, |s| s.hover(move |s| s.bg(hb).border_color(hbd).text_color(ht)))
        }
        Btn::Primary => {
            let grad = p.accent_grad.clone();
            let bright = gpui_kit::linear_gradient(grad.angle, gpui_kit::linear_color_stop(brighten(grad.stops[0].0, 1.08), 0.), gpui_kit::linear_color_stop(brighten(grad.stops[grad.stops.len() - 1].0, 1.08), 1.));
            base.border_color(gpui_kit::transparent_black())
                .bg(grad.background())
                .text_color(p.text_on_accent)
                .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                .when(!disabled, |s| s.hover(move |s| s.bg(bright).text_color(gpui_kit::white())))
        }
        Btn::Danger => {
            let (hb, ht) = (p.danger, gpui_kit::white());
            base.border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text).when(!disabled, |s| s.hover(move |s| s.bg(hb).border_color(hb).text_color(ht)))
        }
    };
    if disabled { base.opacity(0.5) } else { base.cursor_pointer() }
}

/// `.settings-badge(.ok/.off)`: h 26, p 0 10, r 13, `700 11px`.
pub fn badge(label: impl Into<SharedString>, ok: bool, p: &Palette) -> Div {
    div()
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(13.))
        .text_size(fs(11.))
        .line_height(normal_line_height(fs(11.)))
        .font_weight(gpui_kit::FontWeight::BOLD)
        .map(|s| if ok { s.bg(p.success_soft).text_color(p.success_text) } else { s.bg(p.danger_soft).text_color(p.danger_text) })
        .child(label.into())
}

/// `.settings-field`: üstte etiket (`650 10px`, `text-3`), 5px boşluk, altta denetim.
pub fn field(label: impl Into<SharedString>, control: impl IntoElement, p: &Palette) -> Div {
    div().flex().flex_col().gap(px(5.)).child(text(fs(10.)).css_weight(650.).text_color(p.text_3).child(label.into())).child(control)
}

/// Girdi kutusu çerçevesi (`.settings-field input`): h 34, p 0 10, r 8, bg-3, kenarlık; odakta `border-accent` + halka.
pub fn input_frame(focused: bool, p: &Palette) -> Div {
    div()
        .h(px(34.))
        .w_full()
        .flex()
        .items_center()
        .px(px(10.))
        .border_1()
        .rounded(radius::SM)
        .bg(p.bg_3)
        .text_size(fs(12.))
        .text_color(p.text)
        .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
}

const CHEVRON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12"><path d="M2.5 4.5 6 8l3.5-3.5" fill="none" stroke="black" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// `.settings-block select` (kapalı): h 34, p 0 30 0 10, r 8, bg-3, `12px`; sağda 10px içeride #7fb6ff ok.
/// Açıkken odak görünümü (`border-accent` + halka).
pub fn select(id: impl Into<ElementId>, label: impl Into<SharedString>, open: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .h(px(34.))
        .w_full()
        .flex()
        .items_center()
        .pl(px(10.))
        .pr(px(30.))
        .border_1()
        .rounded(radius::SM)
        .bg(p.bg_3)
        .cursor_pointer()
        // Kartın 'dışarı tıklayınca kapat' işleyicisi seçimin kendi tıklamasını yutmasın.
        .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .map(|s| if open { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
        .child(div().flex_1().min_w_0().truncate().text_size(fs(12.)).line_height(normal_line_height(fs(12.))).text_color(p.text).child(label.into()))
        .child(div().absolute().right(px(9.)).top(px(10.)).child(gpui_kit::svg().data(CHEVRON).size(px(12.)).text_color(gpui_kit::rgb(0x7fb6ff))))
}

/// Açık seçimin seçenek listesi (Chromium'un açılır listesi yerine P2 menüsü): seçili satır vurgulu.
/// `pick(i)` tıklanan seçeneğin sırası.
pub fn select_menu<V: 'static>(id: &str, options: &[SharedString], selected: usize, width: f32, p: &Palette, cx: &mut Context<V>, pick: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static) -> AnyElement {
    let pick = Rc::new(pick);
    let (hover_bg, strong) = (p.bg_hover, p.text_strong);
    let mut list = div()
        .id(SharedString::from(format!("{id}-menu")))
        .occlude()
        .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .w(px(width))
        .max_h(px(260.))
        .overflow_y_scroll()
        .mt(px(4.))
        .p(px(4.))
        .flex()
        .flex_col()
        .rounded(radius::SM)
        .border_1()
        .border_color(p.border_strong)
        .bg(p.glass.background())
        .shadow(p.shadow_lg());
    for (i, label) in options.iter().enumerate() {
        let pick = pick.clone();
        let on = i == selected;
        list = list.child(
            div()
                .id(SharedString::from(format!("{id}-opt-{i}")))
                .h(px(28.))
                .px(px(8.))
                .flex()
                .flex_shrink_0()
                .items_center()
                .rounded(radius::XS)
                .text_size(fs(12.))
                .text_color(if on { p.text_strong } else { p.text_2 })
                .when(on, |s| s.bg(p.accent_soft))
                .hover(move |s| s.bg(hover_bg).text_color(strong))
                .cursor_pointer()
                .child(label.clone())
                .on_click(cx.listener(move |this, _, window, cx| pick(this, i, window, cx))),
        );
    }
    gpui_kit::deferred(gpui_kit::anchored().snap_to_window_with_margin(px(8.)).child(list)).with_priority(2).into_any_element()
}

// ------------------------------------------------------------------------------ kaydırıcı

pub type SliderApply<V> = Rc<dyn Fn(&mut V, i32, &mut Context<V>)>;

pub struct SliderSpec<V> {
    pub bounds: Option<Bounds<Pixels>>,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub apply: SliderApply<V>,
}

/// Çizilen kaydırıcıların konumları ve uygulayıcıları (sürükleme için).
pub struct SliderRegistry<V> {
    pub specs: Rc<RefCell<HashMap<SharedString, SliderSpec<V>>>>,
    pub dragging: Option<SharedString>,
}

impl<V: 'static> Default for SliderRegistry<V> {
    fn default() -> Self {
        Self { specs: Rc::default(), dragging: None }
    }
}

impl<V: 'static> SliderRegistry<V> {
    /// İmleç x konumundan değer (thumb merkezi izin sınırları içinde, `step`e yuvarlanır).
    pub fn value_at(&self, id: &SharedString, x: Pixels) -> Option<(i32, SliderApply<V>)> {
        let specs = self.specs.borrow();
        let spec = specs.get(id)?;
        let b = spec.bounds?;
        let w = f32::from(b.size.width) - 14.;
        let frac = ((f32::from(x) - f32::from(b.origin.x) - 7.) / w.max(1.)).clamp(0., 1.);
        let raw = spec.min as f32 + frac * (spec.max - spec.min) as f32;
        let step = spec.step.max(1) as f32;
        let v = ((raw - spec.min as f32) / step).round() * step + spec.min as f32;
        Some(((v.round() as i32).clamp(spec.min, spec.max), spec.apply.clone()))
    }
}

/// `input[type=range]` (settings.css): iz 4px `navy-600` (yarıçap 3; `pill` ise hap), 2px kenar boşluğu;
/// topuz 14px beyaz, `0 1px 4px rgba(0,0,0,.45)`; üzerine gelince ×1.2 ve `0 0 0 4px accent-soft-2` halka.
/// Tıklama/sürükleme değeri `reg` ile bulunan kayıt üzerinden uygular. Sürükleme pencere düzeyindeki
/// fare olaylarıyla izlenir: imleç izden, karttan ya da üstteki bir katmandan (`occlude`) geçse de sürer
/// (eskiden modal kartının `on_mouse_move`u sürüyordu; kart örtülünce — ör. kırpma penceresi — ya da
/// imleç karttan çıkınca hareket hiç gelmiyordu).
#[allow(clippy::too_many_arguments)]
pub fn range<V: 'static>(id: &str, value: i32, min: i32, max: i32, step: i32, pill: bool, p: &Palette, reg: fn(&mut V) -> &mut SliderRegistry<V>, registry: &SliderRegistry<V>, apply: SliderApply<V>, cx: &mut Context<V>) -> AnyElement {
    let key = SharedString::from(id.to_string());
    registry.specs.borrow_mut().insert(key.clone(), SliderSpec { bounds: None, min, max, step, apply });
    let frac = if max > min { ((value - min) as f32 / (max - min) as f32).clamp(0., 1.) } else { 0. };
    let specs = registry.specs.clone();
    let k2 = key.clone();
    let group = SharedString::from(format!("{id}-grp"));
    let ring = p.accent_soft_2;
    let knob_shadow = shadows(&[Shadow::new(0., 1., 4., 0., css_rgba(0, 0, 0, 0.45))]);
    let mut hover_shadow = shadows(&[Shadow::new(0., 0., 0., 4., ring)]);
    hover_shadow.extend(knob_shadow.clone());
    let k3 = key.clone();
    let dragging = registry.dragging.as_ref() == Some(&key);
    let weak = cx.entity().downgrade();
    div()
        .id(SharedString::from(format!("{id}-range")))
        .group(group.clone())
        .relative()
        .h(px(8.))
        .w_full()
        .cursor_pointer()
        .child(
            // Ölçüm: izin gerçek konumu (sürüklemede değer hesabı).
            canvas(
                move |b, _, _| {
                    if let Some(s) = specs.borrow_mut().get_mut(&k2) {
                        s.bounds = Some(b);
                    }
                },
                move |_, _, window, _| {
                    if !dragging {
                        return;
                    }
                    let (w, k) = (weak.clone(), key.clone());
                    window.on_mouse_event(move |e: &gpui_kit::MouseMoveEvent, phase, _, cx| {
                        if phase != gpui_kit::DispatchPhase::Capture {
                            return;
                        }
                        let _ = w.update(cx, |this, cx| {
                            let r = reg(this);
                            if r.dragging.as_ref() != Some(&k) {
                                return;
                            }
                            if e.pressed_button != Some(gpui_kit::MouseButton::Left) {
                                r.dragging = None;
                                cx.notify();
                                return;
                            }
                            if let Some((v, apply)) = r.value_at(&k, e.position.x) {
                                apply(this, v, cx);
                            }
                        });
                    });
                    let w = weak.clone();
                    window.on_mouse_event(move |e: &gpui_kit::MouseUpEvent, phase, _, cx| {
                        if phase == gpui_kit::DispatchPhase::Capture && e.button == gpui_kit::MouseButton::Left {
                            let _ = w.update(cx, |this, cx| {
                                reg(this).dragging = None;
                                cx.notify();
                            });
                        }
                    });
                },
            )
            .absolute()
            .left(px(2.))
            .top(px(2.))
            .w_full()
            .h(px(4.)),
        )
        .child(div().absolute().left(px(2.)).top(px(2.)).w_full().h(px(4.)).rounded(if pill { radius::PILL } else { px(3.) }).bg(p.navy_600))
        .child(
            // Topuz yolu: sol 2px'ten (iz − 14px) boyunca.
            div().absolute().left(px(2.)).right(px(12.)).top(px(-3.)).h(px(14.)).child(
                div()
                    .absolute()
                    .top(px(0.))
                    .left(relative(frac))
                    .size(px(14.))
                    .rounded_full()
                    .bg(gpui_kit::white())
                    .shadow(knob_shadow)
                    .group_hover(group, move |s| s.size(px(16.8)).mt(px(-1.4)).ml(px(-1.4)).shadow(hover_shadow.clone())),
            ),
        )
        .on_mouse_down(
            gpui_kit::MouseButton::Left,
            cx.listener(move |this, e: &gpui_kit::MouseDownEvent, _, cx| {
                let r = reg(this);
                if let Some((v, apply)) = r.value_at(&k3, e.position.x) {
                    r.dragging = Some(k3.clone());
                    apply(this, v, cx);
                }
                // Topuzun tam ortasına basınca değer değişmez: sürükleme dinleyicileri için yine de yeniden çiz.
                cx.notify();
                cx.stop_propagation();
            }),
        )
        .into_any_element()
}

/// `.settings-slider`: üstte etiket ve sağda değer (`650 10px` / `900 10px`), 6px, altta kaydırıcı.
#[allow(clippy::too_many_arguments)]
pub fn slider<V: 'static>(id: &str, label: impl Into<SharedString>, value: i32, shown: String, min: i32, max: i32, step: i32, p: &Palette, reg: fn(&mut V) -> &mut SliderRegistry<V>, registry: &SliderRegistry<V>, apply: SliderApply<V>, cx: &mut Context<V>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(text(fs(10.)).css_weight(650.).text_color(p.text_3).child(label.into()))
                .child(text(fs(10.)).css_weight(900.).text_color(p.text).child(shown)),
        )
        .child(range(id, value, min, max, step, false, p, reg, registry, apply, cx))
}

/// Türkçe küçük harf (I → ı, İ → i).
pub fn lower_tr(s: &str) -> String {
    s.chars().map(|c| match c {
        'I' => "ı".to_string(),
        'İ' => "i".to_string(),
        c => c.to_lowercase().to_string(),
    }).collect()
}

/// Bilinen seçim listeleri (otomatik pilot `select:<ad>`).
pub fn select_name(name: &str) -> Option<&'static str> {
    ["font", "zone", "week", "in-dev", "out-dev", "cam", "bitrate", "bandwidth", "corner", "hudpos"].into_iter().find(|n| *n == name)
}
