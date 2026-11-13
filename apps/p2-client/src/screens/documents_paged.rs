//! Sayfalı doküman — DocumentsPage.tsx `PagedEditor` + documents.css'in birebir görünümü.
//!
//! Kâğıt (A3/A4/A5/özel, mm → px 96 dpi), üst ve sol cetvel (10 mm / 2 mm çentikleri, kenar boşluğu
//! tutamaçları), koyu kâğıt (`--doc-surface`), Arial 14px×0,85 / 1,75 satır, sayfa numarası; sağda
//! "Sayfa araçları" paneli. Electron'daki contentEditable yerine: kâğıda tıklayınca o sayfa biçimli metin
//! kipinde (`# başlık`, `**kalın**` …) düzenlenir; araç düğmeleri biçim işaretlerini ekler.

use super::*;
use p2_ui::P2Transition as _; // kendi kaydırma çubuğu var (otomatik çubuk kapalı)

/// 1 mm = 96/25,4 px (CSS).
pub(super) const MM: f32 = 3.779_527_6;

/// Sayfa ölçüsü (mm) ve kenar boşlukları (üst, sağ, alt, sol).
pub(super) fn page_geometry(content: &Value) -> (String, f32, f32, [f32; 4]) {
    let preset = content["pageSize"].as_str().unwrap_or("A4").to_string();
    let (pw, ph) = match preset.as_str() {
        "A3" => (297., 420.),
        "A5" => (148., 210.),
        _ => (210., 297.),
    };
    let w = content["pageWidthMm"].as_f64().map(|v| v as f32).unwrap_or(pw).clamp(100., 500.);
    let h = content["pageHeightMm"].as_f64().map(|v| v as f32).unwrap_or(ph).clamp(140., 700.);
    let m = &content["margins"];
    let g = |k: &str| m[k].as_f64().map(|v| v as f32).unwrap_or(20.);
    (preset, w, h, [g("top"), g("right"), g("bottom"), g("left")])
}

/// Kâğıttaki bir blok: Electron'daki h1/h2/h3/p/blockquote kuralları (Arial, satır 1,75, em kenar boşlukları).
fn paper_block(b: &Block, p: &Palette) -> (AnyElement, f32) {
    let text = b.text();
    let mut highlights = Vec::new();
    let mut at = 0;
    for s in &b.spans {
        let end = at + s.text.len();
        if s.bold || s.italic || s.underline || s.link.is_some() {
            highlights.push((
                at..end,
                HighlightStyle {
                    font_weight: s.bold.then_some(FontWeight::BOLD),
                    font_style: s.italic.then_some(FontStyle::Italic),
                    underline: (s.underline || s.link.is_some()).then(|| UnderlineStyle { thickness: px(1.), ..Default::default() }),
                    color: s.link.as_ref().map(|_| p.link),
                    ..Default::default()
                },
            ));
        }
        at = end;
    }
    let base = fs(14.);
    let (size, weight, margin) = match b.kind {
        BlockKind::H1 => (fs(30.), FontWeight::BOLD, f32::from(fs(30.)) * 0.67),
        BlockKind::H2 => (fs(24.), FontWeight::BOLD, f32::from(fs(24.)) * 0.83),
        BlockKind::H3 => (fs(19.), FontWeight::BOLD, f32::from(fs(19.))),
        _ => (base, FontWeight::NORMAL, f32::from(base)),
    };
    let mut el = div().w_full().text_size(size).line_height(px(f32::from(size) * 1.75)).font_weight(weight).text_color(p.text).min_h(px(f32::from(size) * 1.75));
    el = match b.align {
        Align::Center => el.text_center(),
        Align::Right => el.text_right(),
        Align::Left => el,
    };
    if b.kind == BlockKind::Quote {
        el = el.pl(px(15.)).mr(px(40.)).border_l(px(3.)).border_color(p.accent_hover).text_color(p.text_3);
    }
    (el.child(StyledText::new(if text.is_empty() { " ".to_string() } else { text }).with_highlights(highlights)).into_any_element(), margin)
}

/// Bloklar arası kenar boşluğu çökmesi (CSS margin collapse) ile kâğıt içeriği.
fn paper_body(body: &str, p: &Palette) -> AnyElement {
    let (blocks, _) = html_to_blocks(body);
    let mut col = div().w_full().flex().flex_col().font_family("Arial");
    let mut prev = 0.;
    for (i, b) in blocks.iter().enumerate() {
        let (el, m) = paper_block(b, p);
        let top = if i == 0 { m } else { m.max(prev) };
        col = col.child(div().w_full().mt(px(top)).child(el));
        prev = m;
    }
    col.into_any_element()
}

/// Cetvel çentikleri: `repeating-linear-gradient` (10 mm ana, 2 mm ara, 1px çizgi).
fn ruler_ticks(horizontal: bool, length: f32, p: &Palette) -> AnyElement {
    let major = p.border;
    let mut minor = p.border;
    minor.a *= 0.55;
    canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
        let o = b.origin;
        let thick = if horizontal { f32::from(b.size.height) } else { f32::from(b.size.width) };
        for (step, color, extent) in [(2. * MM, minor, thick - 1.), (10. * MM, major, thick)] {
            let mut v = step - 1.;
            while v < length {
                let r = if horizontal { Bounds::new(o + point(px(v), px(0.)), gpui_kit::size(px(1.), px(extent))) } else { Bounds::new(o + point(px(0.), px(v)), gpui_kit::size(px(extent), px(1.))) };
                window.paint_quad(gpui_kit::fill(r, color));
                v += step;
            }
        }
    })
    .absolute()
    .inset_0()
    .into_any_element()
}

/// Kenar boşluğu tutamacı: accent-hover renginde 8×7 (yatay) / 7×8 (dikey) üçgen.
fn ruler_handle(horizontal: bool, at: f32, color: Hsla) -> AnyElement {
    let tri = canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
        let o = b.origin;
        let mut t = PathBuilder::fill();
        if horizontal {
            t.move_to(o);
            t.line_to(o + point(px(8.), px(0.)));
            t.line_to(o + point(px(4.), px(7.)));
        } else {
            t.move_to(o);
            t.line_to(o + point(px(7.), px(4.)));
            t.line_to(o + point(px(0.), px(8.)));
        }
        t.close();
        if let Ok(path) = t.build() {
            window.paint_path(path, color);
        }
    });
    if horizontal {
        div().absolute().left(px(at - 6. + 2.)).top(px(1.)).w(px(8.)).h(px(7.)).child(tri.size_full()).into_any_element()
    } else {
        div().absolute().left(px(1.)).top(px(at - 6. + 2.)).w(px(7.)).h(px(8.)).child(tri.size_full()).into_any_element()
    }
}

/// Chromium kaydırma çubukları (10px, başparmak rgba(120,170,255,.18) hap, 2px iç boşluk); iki eksen
/// birden taşınca sağ altta beyaz köşe kutusu (Electron'da scrollbar-corner varsayılanı).
pub(super) fn scrollbars(h: &gpui_kit::ScrollHandle, corner: bool) -> Vec<AnyElement> {
    let b = h.bounds();
    let max = h.max_offset();
    let off = h.offset();
    let (vw, vh) = (f32::from(b.size.width), f32::from(b.size.height));
    let (mx, my) = (f32::from(max.x), f32::from(max.y));
    let (both_x, both_y) = (mx > 0.5, my > 0.5);
    let thumb_color = p2_ui::tokens::css_rgba(120, 170, 255, 0.18);
    let mut out = Vec::new();
    if both_y && vh > 0. {
        let track = vh - if both_x { 10. } else { 0. };
        let thumb = (track * track / (track + my)).max(20.);
        let top = -f32::from(off.y) / my * (track - thumb);
        out.push(div().absolute().top_0().right_0().w(px(10.)).h(px(track)).child(div().absolute().top(px(top + 2.)).left(px(2.)).w(px(6.)).h(px(thumb - 4.)).rounded(radius::PILL).bg(thumb_color)).into_any_element());
    }
    if both_x && vw > 0. {
        let track = vw - if both_y { 10. } else { 0. };
        let thumb = (track * track / (track + mx)).max(20.);
        let left = -f32::from(off.x) / mx * (track - thumb);
        out.push(div().absolute().left_0().bottom_0().h(px(10.)).w(px(track)).child(div().absolute().left(px(left + 2.)).top(px(2.)).h(px(6.)).w(px(thumb - 4.)).rounded(radius::PILL).bg(thumb_color)).into_any_element());
    }
    if corner && both_x && both_y {
        out.push(div().absolute().right_0().bottom_0().size(px(10.)).bg(gpui_kit::white()).into_any_element());
    }
    out
}

impl DocumentsPage {
    /// Sayfalar: sol tarafta kaydırılabilir kâğıtlar, sağda araç paneli.
    pub(super) fn paged_view(&self, writable: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = &self.doc else { return div().into_any_element() };
        let (preset, wmm, hmm, margins) = page_geometry(&doc.content);
        let (w, h) = (wmm * MM, hmm * MM);
        let bodies = page_bodies(&doc.content);
        let count = match &self.pages {
            Some(e) => e.len(),
            None => bodies.len().max(1),
        };
        let active = self.active_page.min(count.saturating_sub(1));
        let mut sheets = div().flex().flex_col().gap(px(34.));
        for i in 0..count {
            let is_active = i == active;
            let mut border = p.border_subtle;
            if is_active {
                // color-mix(accent-hover 55%, line)
                border = gpui_kit::hsla(p.accent_hover.h, p.accent_hover.s, p.accent_hover.l, 0.596);
            }
            let content: AnyElement = match &self.pages {
                Some(editors) if i < editors.len() => Textarea::new(&editors[i]).appearance(false).text_size(fs(14.)).text_color(p.text).font_family("Arial").into_any_element(),
                _ => paper_body(bodies.get(i).map(String::as_str).unwrap_or(""), p),
            };
            let mut glow = p.accent_hover;
            glow.a = 0.24;
            let paper = div()
                .id(SharedString::from(format!("paper-{i}")))
                .absolute()
                .left(px(24.))
                .top(px(24.))
                .w(px(w))
                .h(px(h))
                .pt(px(margins[0] * MM))
                .pr(px(margins[1] * MM))
                .pb(px(margins[2] * MM))
                .pl(px(margins[3] * MM))
                .overflow_hidden()
                .rounded(radius::XS)
                .border_1()
                .border_color(border)
                .bg(p.bg_1)
                .when(is_active, |s| {
                    s.shadow(vec![
                        gpui_kit::BoxShadow { color: gpui_kit::hsla(0., 0., 0., 0.3), offset: point(px(0.), px(18.)), blur_radius: px(65.), spread_radius: px(0.), inset: false },
                        gpui_kit::BoxShadow { color: glow, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false },
                    ])
                })
                .when(writable && self.lossless, |s| s.cursor_text())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.active_page = i;
                    this.edit_pages(window, cx);
                    if let Some(e) = this.pages.as_ref().and_then(|p| p.get(i)).cloned() {
                        e.update(cx, |t, cx| t.focus(window, cx));
                    }
                    cx.notify();
                }))
                .child(content)
                .child(div().absolute().right(px(9.)).bottom(px(7.)).text_size(fs(8.)).text_color(p.text_3).child(format!("{} / {}", i + 1, count)));
            let horizontal = div()
                .absolute()
                .left(px(24.))
                .top(px(0.))
                .w(px(w))
                .h(px(20.))
                .border_b_1()
                .border_color(p.border)
                .opacity(0.8)
                .text_size(fs(7.))
                .text_color(p.text_3)
                .child(ruler_ticks(true, w, p))
                .child(div().absolute().left(px(2.)).bottom(px(2.)).child("0"))
                .child(div().absolute().right(px(2.)).bottom(px(2.)).child(format!("{} mm", wmm.round())))
                .child(ruler_handle(true, margins[3] * MM, p.accent_hover))
                .child(ruler_handle(true, w - margins[1] * MM, p.accent_hover));
            let vertical = div()
                .absolute()
                .left(px(0.))
                .top(px(24.))
                .w(px(20.))
                .h(px(h))
                .border_r_1()
                .border_color(p.border)
                .opacity(0.8)
                .text_size(fs(7.))
                .text_color(p.text_3)
                .child(ruler_ticks(false, h, p))
                .child(div().absolute().right(px(3.)).top(px(2.)).child("0"))
                .child(div().absolute().right(px(2.)).bottom(px(3.)).flex().flex_col().items_center().children(format!("{}", hmm.round()).chars().map(|c| div().line_height(px(3.6)).child(c.to_string()))))
                .child(ruler_handle(false, margins[0] * MM, p.accent_hover))
                .child(ruler_handle(false, h - margins[2] * MM, p.accent_hover));
            sheets = sheets.child(div().relative().flex_shrink_0().w(px(w + 24.)).h(px(h + 24.)).child(horizontal).child(vertical).child(paper));
        }
        let surface = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(p.bg_0)
            .child(div().id("page-surface").size_full().overflow_scroll().track_scroll(&self.page_scroll).no_auto_scrollbar().on_scroll(cx.listener(|_, _, _, cx| cx.notify())).child(div().w_full().min_w(px(w + 24. + 96.)).pt(px(38.)).px(px(48.)).pb(px(70.)).flex().flex_col().items_center().child(sheets)))
            .children(scrollbars(&self.page_scroll, true));
        let _ = preset;
        div().flex_1().min_h_0().w_full().flex().child(surface).child(self.page_tools(writable, count, p, cx)).into_any_element()
    }

    /// `.documents-page-tools`: 232px, bölümler (Sayfa, Yazı, Biçim, Paragraf, Dosya ve medya, Düzenle).
    fn page_tools(&self, writable: bool, count: usize, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let bars = scrollbars(&self.tools_scroll, false);
        // İlk çizimde kaydırma ölçüleri henüz yok: bir kare sonra yeniden çiz (çubuklar ve 10px pay).
        if f32::from(self.tools_scroll.bounds().size.width) == 0. || f32::from(self.page_scroll.bounds().size.width) == 0. {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(30)).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        let panel = self.page_tools_panel(writable, count, p, cx);
        div().relative().w(px(232.)).flex_shrink_0().h_full().border_l_1().border_color(p.border_subtle).bg(p.bg_1).child(panel).children(bars).into_any_element()
    }

    fn page_tools_panel(&self, writable: bool, count: usize, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let content = self.doc.as_ref().map(|d| d.content.clone()).unwrap_or(Value::Null);
        let (preset, wmm, hmm, _) = page_geometry(&content);
        let kicker = |t: &str| div().h(px(10.)).text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child(tr_upper(t));
        let section = || div().mb(px(10.)).p(px(9.)).flex().flex_col().gap(px(7.)).rounded(radius::MD).border_1().border_color(p.border_subtle).bg(p.bg_2);
        let (hb, hbg) = (p.border_accent, p.accent_soft);
        let select = |id: String, label: String| {
            div()
                .id(SharedString::from(id))
                .h(px(31.))
                .w_full()
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .text_size(font_size::MD)
                .text_color(p.text)
                .cursor_pointer()
                .child(div().flex_1().pl(px(4.)).child(label))
                .child(p2_ui::glyphs::ui_stroke("chevron-down", 12., 2.4).text_color(p.text))
        };
        let tool = |id: String, label: AnyElement, h: f32, enabled: bool| {
            div()
                .id(SharedString::from(id))
                .flex_1()
                .h(px(h))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .text_color(p.text)
                .text_size(font_size::MD)
                .when(enabled, move |s| s.cursor_pointer().hover(move |s| s.border_color(hb).bg(hbg)))
                .when(!enabled, |s| s.opacity(0.4))
                .child(label)
        };
        let row = |items: Vec<AnyElement>| div().flex().gap(px(5.)).children(items);
        let size_label = match preset.as_str() {
            "A3" => "A3 · 297 × 420 mm".to_string(),
            "A5" => "A5 · 148 × 210 mm".to_string(),
            "CUSTOM" => format!("Özel ölçü · {wmm:.0} × {hmm:.0}"),
            _ => "A4 · 210 × 297 mm".to_string(),
        };
        let fmt = |id: &'static str, label: AnyElement, token: &'static str, cx: &mut Context<Self>| tool(id.into(), label, 31., writable).on_click(cx.listener(move |this, _, window, cx| this.page_mark(token, window, cx))).into_any_element();
        let presets = ["A3", "A4", "A5", "CUSTOM"];
        let next_preset = presets[(presets.iter().position(|x| *x == preset).unwrap_or(1) + 1) % presets.len()];
        div()
            .id("page-tools")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.tools_scroll).no_auto_scrollbar()
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .p(px(12.))
            .when(f32::from(self.tools_scroll.max_offset().y) > 0.5, |s| s.pr(px(22.)))
            .text_color(p.text)
            .child(
                div()
                    .pt(px(3.))
                    .px(px(2.))
                    .pb(px(11.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(fs(11.)).font_weight(FontWeight::BOLD).child("Sayfa araçları"))
                    .child(kicker(&format!("{count} sayfa"))),
            )
            .child(
                section()
                    .child(kicker("Sayfa"))
                    .child(select("pt-size".into(), size_label).when(writable, |s| s.on_click(cx.listener(move |this, _, window, cx| this.page_preset(next_preset, window, cx)))))
                    .child(row(vec![
                        tool("pt-add".into(), div().text_size(fs(8.)).child("＋ Sayfa ekle").into_any_element(), 30., writable).on_click(cx.listener(|this, _, window, cx| this.add_page(window, cx))).into_any_element(),
                        tool("pt-del".into(), div().text_size(fs(8.)).child("Sayfayı sil").into_any_element(), 30., writable && count > 1).on_click(cx.listener(|this, _, window, cx| this.remove_page(window, cx))).into_any_element(),
                    ])),
            )
            .child(
                section()
                    .child(kicker("Yazı"))
                    .child(select("pt-block".into(), "Normal".into()).on_click(cx.listener(|this, _, window, cx| this.page_mark("# ", window, cx))))
                    .child(select("pt-font".into(), "Arial".into()))
                    .child(select("pt-pt".into(), "14 punto".into())),
            )
            .child(
                section()
                    .child(kicker("Biçim"))
                    .child(row(vec![
                        fmt("pt-b", div().font_weight(FontWeight::BOLD).child("B").into_any_element(), "**", cx),
                        fmt("pt-i", div().italic().child("I").into_any_element(), "*", cx),
                        fmt("pt-u", div().underline().child("U").into_any_element(), "__", cx),
                        fmt("pt-s", div().line_through().child("S").into_any_element(), "~~", cx),
                    ]))
                    .child(row(vec![
                        tool("pt-color".into(), div().flex().flex_col().items_center().w_full().child(div().text_size(fs(11.)).child("A")).child(div().mt(px(3.)).mx(px(4.)).w(px(76.)).h(px(5.)).bg(rgb(0x6b6b6b))).into_any_element(), 32., writable).into_any_element(),
                        tool("pt-hl".into(), div().flex().flex_col().items_center().w_full().child(div().text_size(fs(11.)).child("▰")).child(div().mt(px(3.)).mx(px(4.)).w(px(76.)).h(px(5.)).bg(rgb(0x6b6b6b))).into_any_element(), 32., writable).into_any_element(),
                    ])),
            )
            .child(
                section().child(kicker("Paragraf")).child(row(vec![fmt("pt-al", div().child("≡").into_any_element(), "[sol]", cx), fmt("pt-ac", div().child("≣").into_any_element(), "[orta]", cx), fmt("pt-ar", div().child("≡").into_any_element(), "[sağ]", cx), fmt("pt-aj", div().child("☰").into_any_element(), "", cx)])).child(row(vec![
                    fmt("pt-ul", div().child("•").into_any_element(), "- ", cx),
                    fmt("pt-ol", div().child("1.").into_any_element(), "1. ", cx),
                    fmt("pt-out", div().child("←").into_any_element(), "", cx),
                    fmt("pt-in", div().child("→").into_any_element(), "> ", cx),
                ])),
            )
            .child(section().child(kicker("Dosya ve medya")).child(tool("pt-file".into(), div().flex().flex_col().items_center().gap(px(2.)).child("＋").child(div().text_size(fs(8.)).child("Her tür dosya ekle")).into_any_element(), 42., writable)))
            .child(section().child(kicker("Düzenle")).child(row(vec![
                fmt("pt-link", div().font_family(p2_ui::tokens::FONT_EMOJI).child("🔗").into_any_element(), "[metin](https://)", cx),
                fmt("pt-clear", div().child("Tx").into_any_element(), "", cx),
                fmt("pt-undo", div().child("↶").into_any_element(), "", cx),
                fmt("pt-redo", div().child("↷").into_any_element(), "", cx),
            ])))
            .into_any_element()
    }

    /// Biçim düğmesi: düzenleme kipine geçip etkin sayfanın sonuna biçim işaretini ekler.
    fn page_mark(&mut self, token: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if token.is_empty() {
            return;
        }
        self.edit_pages(window, cx);
        let i = self.active_page;
        if let Some(e) = self.pages.as_ref().and_then(|p| p.get(i)).cloned() {
            e.update(cx, |t, cx| {
                let text = format!("{}{}", t.value(), token);
                t.set_value(text, window, cx);
                t.focus(window, cx);
            });
            self.schedule_save(window, cx);
        }
    }

    fn page_preset(&mut self, preset: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        let (_, w, h, _) = page_geometry(&doc.content);
        let (pw, ph) = match preset {
            "A3" => (297., 420.),
            "A5" => (148., 210.),
            "CUSTOM" => (w, h),
            _ => (210., 297.),
        };
        if let Some(obj) = doc.content.as_object_mut() {
            obj.insert("pageSize".into(), json!(preset));
            obj.insert("pageWidthMm".into(), json!(pw));
            obj.insert("pageHeightMm".into(), json!(ph));
            obj.insert("version".into(), json!(3));
        }
        self.schedule_save(window, cx);
    }

    fn remove_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_pages(window, cx);
        let i = self.active_page;
        if let Some(pages) = &mut self.pages {
            if pages.len() > 1 && i < pages.len() {
                pages.remove(i);
                self.active_page = i.saturating_sub(1);
                self.schedule_save(window, cx);
            }
        }
    }
}
