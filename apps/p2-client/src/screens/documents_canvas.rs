//! Serbest tuval — DocumentsPage.tsx `CanvasEditor` + documents.css'in birebir karşılığı.
//!
//! Dünya koordinatı (Electron'daki `ORIGIN` merkezli 30000×20000 alan) → ekran: `(dünya − görünüm) × yakınlık`.
//! İlk açılışta içerik merkeze alınır (Electron `contentCenter`), yakınlık 1. Izgara (nokta/çizgi/yok), düğümler
//! (kutu, küre, üçgen, düz metin, dosya), kenarlık, bağlantı noktaları, Electron'un dik açılı yol bulucusuyla
//! çizilen bağlantılar (ok, orta nokta, etiket), seçim çerçevesi + boyutlandırma tutamaçları, sağdaki araç kutusu,
//! mini harita, boş tuval yazısı, sağ tık menüleri (düğüm / hızlı araçlar), geri al / yinele, kopyala / yapıştır.

use std::cell::Cell;
use p2_ui::P2Fx as _; // backdrop_blur
use std::rc::Rc;

use gpui_kit::{BorderStyle, Bounds, Corners, Edges, KeyDownEvent, MouseUpEvent, TextRun, font, quad, size};
use p2_client_core::CanvasWire;

use super::*;

pub(super) const SNAP: f32 = 20.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub(super) fn parse(s: Option<&str>) -> Side {
        match s.unwrap_or("right") {
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            "left" => Side::Left,
            _ => Side::Right,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Right => "right",
            Side::Bottom => "bottom",
            Side::Left => "left",
        }
    }
    const ALL: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct P {
    pub x: f32,
    pub y: f32,
}

fn pt(x: f32, y: f32) -> P {
    P { x, y }
}

#[derive(Clone)]
pub(super) enum CanvasDrag {
    None,
    /// Sağ tuşla gezinme (Electron `startPan`): başlangıç ekran noktası + görünüm.
    Pan { from: Point<Pixels>, view: P, moved: bool },
    /// Düğüm(ler)i taşıma: dünya başlangıcı + özgün konumlar.
    Move { start: P, originals: Vec<(String, f32, f32)>, remembered: bool },
    /// Köşeden boyutlandırma.
    Resize { id: String, dir: &'static str, start: Point<Pixels>, node: CanvasNode },
    /// Seçim kutusu.
    Marquee { start: P, base: Vec<String> },
    /// Bağlantı çekme (bağlantı noktasından).
    Wire { from: String, side: Side, at: P },
}

#[derive(Clone)]
pub(super) enum CanvasMenu {
    Node { id: String, at: Point<Pixels> },
    Canvas { at: Point<Pixels>, world: P },
}

pub(super) struct CanvasView {
    pub content: CanvasContent,
    pub zoom: f32,
    /// Görünür alanın sol üst köşesinin dünya koordinatı.
    pub view: P,
    pub centered: bool,
    pub selection: Vec<String>,
    pub selected_wire: Option<String>,
    pub editing: Option<(String, Entity<TextareaState>)>,
    pub drag: CanvasDrag,
    pub undo: Vec<CanvasContent>,
    pub redo: Vec<CanvasContent>,
    pub clipboard: Option<(Vec<CanvasNode>, Vec<CanvasWire>)>,
    pub marquee: Option<(f32, f32, f32, f32)>,
    pub menu: Option<CanvasMenu>,
    pub cursor: Option<P>,
    /// Kaydırma alanının pencere içindeki sınırları (son çizimden).
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
    pub format_suppressed: bool,
    /// Otomatik pilot: ilk çizimde ortada hızlı araçlar menüsü aç.
    pub pending_menu: bool,
}

impl CanvasView {
    pub(super) fn new(content: CanvasContent) -> Self {
        Self {
            content,
            zoom: 1.,
            view: pt(-450., -300.),
            centered: false,
            selection: Vec::new(),
            selected_wire: None,
            editing: None,
            drag: CanvasDrag::None,
            undo: Vec::new(),
            redo: Vec::new(),
            clipboard: None,
            marquee: None,
            menu: None,
            cursor: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            format_suppressed: false,
            pending_menu: false,
        }
    }

    fn remember(&mut self) {
        self.undo.push(self.content.clone());
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Ekran (pencere) noktasını dünya koordinatına çevirir.
    fn world(&self, at: Point<Pixels>) -> P {
        let b = self.bounds.get();
        pt(f32::from(at.x - b.origin.x) / self.zoom + self.view.x, f32::from(at.y - b.origin.y) / self.zoom + self.view.y)
    }

    fn node(&self, id: &str) -> Option<&CanvasNode> {
        self.content.nodes.iter().find(|n| n.id == id)
    }

    fn grid_mode(&self) -> String {
        self.content.extra.get("grid").and_then(|g| g.as_str()).unwrap_or("DOTS").to_string()
    }
}

// ------------------------------------------------------------------ düğüm alanları

fn num(n: &CanvasNode, key: &str) -> Option<f32> {
    n.extra.get(key).and_then(|v| v.as_f64()).map(|v| v as f32)
}
fn text_of(n: &CanvasNode, key: &str) -> Option<String> {
    n.extra.get(key).and_then(|v| v.as_str()).map(str::to_string)
}
fn flag(n: &CanvasNode, key: &str) -> bool {
    n.extra.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}
fn kind(n: &CanvasNode) -> &str {
    if n.kind == "NOTE" { "BOX" } else { n.kind.as_str() }
}

// -------------------------------------------------------------- şekil geometrisi

/// Elipsin çokgen yaklaşımındaki parça sayısı (seçim çizgisi ve isabet testi).
const ELLIPSE_SEGMENTS: usize = 64;

/// Şeklin yerel koordinatlardaki (0..w, 0..h) dış hattı, `grow` kadar dışarı büyütülmüş.
/// Üçgen: üç köşe (gönye birleşimli ofset); küre: elips; diğerleri (kutu, metin, dosya): dikdörtgen.
/// Seçim vurgusu bu hattı çizer, isabet testi de aynı hattı kullanır (sınır kutusu değil).
pub(super) fn shape_outline(kind: &str, w: f32, h: f32, grow: f32) -> Vec<(f32, f32)> {
    match kind {
        "TRIANGLE" => offset_polygon(&[(w / 2., 0.), (w, h), (0., h)], grow),
        "SPHERE" => {
            let (cx, cy, rx, ry) = (w / 2., h / 2., (w / 2. + grow).max(0.), (h / 2. + grow).max(0.));
            (0..ELLIPSE_SEGMENTS)
                .map(|i| {
                    let a = i as f32 / ELLIPSE_SEGMENTS as f32 * std::f32::consts::TAU;
                    (cx + rx * a.cos(), cy + ry * a.sin())
                })
                .collect()
        }
        _ => vec![(-grow, -grow), (w + grow, -grow), (w + grow, h + grow), (-grow, h + grow)],
    }
}

/// Dışbükey çokgeni her kenarından `d` kadar dışarı öteler (köşelerde gönye birleşimi).
fn offset_polygon(pts: &[(f32, f32)], d: f32) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 3 || d == 0. {
        return pts.to_vec();
    }
    // Yön: işaretli alan > 0 ise (y aşağı ekran düzleminde) saat yönü; dış normal buna göre seçilir.
    let area: f32 = (0..n).map(|i| {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        a.0 * b.1 - b.0 * a.1
    }).sum();
    let sign = if area >= 0. { 1. } else { -1. };
    let normal = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy).max(f32::EPSILON);
        (sign * dy / len, -sign * dx / len)
    };
    (0..n)
        .map(|i| {
            let (prev, cur, next) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let (n1, n2) = (normal(prev, cur), normal(cur, next));
            // Gönye: iki normalin ortalaması yönünde d / cos(θ/2).
            let k = d / (1. + n1.0 * n2.0 + n1.1 * n2.1).max(0.05);
            (cur.0 + (n1.0 + n2.0) * k, cur.1 + (n1.1 + n2.1) * k)
        })
        .collect()
}

/// Çift-tek kuralıyla nokta-çokgen içinde testi (kenar üstü dahil sayılabilir).
pub(super) fn point_in_polygon(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let ((xi, yi), (xj, yj)) = (poly[i], poly[j]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Yerel noktanın (düğüm sol üstüne göre) şeklin içinde olup olmadığı; `tol` kadar pay tanınır.
pub(super) fn shape_contains(kind: &str, w: f32, h: f32, x: f32, y: f32, tol: f32) -> bool {
    match kind {
        "SPHERE" => {
            let (rx, ry) = ((w / 2. + tol).max(f32::EPSILON), (h / 2. + tol).max(f32::EPSILON));
            let (dx, dy) = ((x - w / 2.) / rx, (y - h / 2.) / ry);
            dx * dx + dy * dy <= 1.
        }
        "TRIANGLE" => point_in_polygon(&shape_outline(kind, w, h, tol), x, y),
        _ => x >= -tol && x <= w + tol && y >= -tol && y <= h + tol,
    }
}

/// Çokgen ile eksene hizalı dikdörtgen (x, y, w, h) kesişiyor mu (seçim kutusu için).
pub(super) fn polygon_intersects_rect(poly: &[(f32, f32)], r: (f32, f32, f32, f32)) -> bool {
    let (rx0, ry0, rx1, ry1) = (r.0, r.1, r.0 + r.2, r.1 + r.3);
    let inside_rect = |p: &(f32, f32)| p.0 >= rx0 && p.0 <= rx1 && p.1 >= ry0 && p.1 <= ry1;
    if poly.iter().any(inside_rect) {
        return true;
    }
    let corners = [(rx0, ry0), (rx1, ry0), (rx1, ry1), (rx0, ry1)];
    if corners.iter().any(|c| point_in_polygon(poly, c.0, c.1)) {
        return true;
    }
    // Köşeler birbirinin içinde değilse yalnız kenar kesişmesi kalır.
    let cross = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
    let seg = |a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)| {
        let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
        (d1 > 0.) != (d2 > 0.) && (d3 > 0.) != (d4 > 0.)
    };
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        (0..4).any(|j| seg(a, b, corners[j], corners[(j + 1) % 4]))
    })
}

/// Dünya noktası düğümün şekline isabet ediyor mu (sınır kutusu değil, gerçek şekil).
fn node_hit(n: &CanvasNode, at: P, tol: f32) -> bool {
    shape_contains(kind(n), n.w as f32, n.h as f32, at.x - n.x as f32, at.y - n.y as f32, tol)
}

/// Seçim vurgusu renkleri: (ana çizgi, alt çizgi). Mavi şekillerin üstünde de seçilsin diye nötr gri:
/// koyu temada açık gri + ince koyu alt çizgi, açık temada koyu gri + ince açık alt çizgi.
pub(super) fn selection_colors(p: &Palette) -> (Hsla, Hsla) {
    if p.dark {
        (rgb(0xd9dce3).into(), gpui_kit::hsla(0., 0., 0.04, 0.85))
    } else {
        (rgb(0x4b505e).into(), gpui_kit::hsla(0., 0., 1., 0.92))
    }
}

/// Kapalı çokgeni `origin`e göre çizgi olarak boyar.
fn stroke_polygon(window: &mut Window, origin: Point<Pixels>, pts: &[(f32, f32)], width: f32, color: Hsla) {
    let Some(first) = pts.first() else { return };
    let mut b = PathBuilder::stroke(px(width));
    b.move_to(origin + point(px(first.0), px(first.1)));
    for (x, y) in &pts[1..] {
        b.line_to(origin + point(px(*x), px(*y)));
    }
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}
fn wire_side(w: &CanvasWire, key: &str) -> Side {
    Side::parse(w.extra.get(key).and_then(|v| v.as_str()))
}
fn wire_points(w: &CanvasWire) -> Vec<P> {
    w.extra.get("points").and_then(|v| v.as_array()).map(|a| a.iter().map(|p| pt(p["x"].as_f64().unwrap_or(0.) as f32, p["y"].as_f64().unwrap_or(0.) as f32)).collect()).unwrap_or_default()
}

/// CSS renk dizgisi (`#rgb`, `#rrggbb`, `transparent`).
fn css_color(s: Option<&str>, fallback: Hsla) -> Hsla {
    match s {
        Some("transparent") => gpui_kit::transparent_black(),
        other => parse_color(other).unwrap_or(fallback),
    }
}

/// DocumentsPage.tsx `textBounds`: düz metin düğümünün ölçüsü (`sans-serif` = Arial ile ölçülür).
pub(super) fn text_bounds(n: &CanvasNode, window: &Window) -> (f32, f32) {
    let size = num(n, "fontSize").unwrap_or(18.);
    let line_height = size * 1.45;
    let text = n.text.clone().unwrap_or_default();
    let lines: Vec<&str> = text.split('\n').collect();
    let mut f = font(text_of(n, "fontFamily").filter(|f| f != "inherit").unwrap_or_else(|| "Arial".into()).trim_matches('\'').to_string());
    f.weight = if flag(n, "bold") { FontWeight::EXTRA_BOLD } else { FontWeight::MEDIUM };
    if flag(n, "italic") {
        f.style = FontStyle::Italic;
    }
    let width = lines
        .iter()
        .map(|line| {
            let line = if line.is_empty() { " " } else { line };
            let run = TextRun { len: line.len(), font: f.clone(), color: gpui_kit::black(), background_color: None, underline: None, strikethrough: None };
            f32::from(window.text_system().shape_line(SharedString::from(line.to_string()), px(size), &[run], None).width)
        })
        .fold(0., f32::max);
    let w = if text.is_empty() { 140. } else { (width + 12.).ceil().max(24.) };
    let h = (line_height + 10.).ceil().max((lines.len() as f32 * line_height + 10.).ceil());
    (w, h)
}

fn side_point(n: &CanvasNode, s: Side) -> P {
    let (x, y, w, h) = (n.x as f32, n.y as f32, n.w as f32, n.h as f32);
    match s {
        Side::Top => pt(x + w / 2., y),
        Side::Right => pt(x + w, y + h / 2.),
        Side::Bottom => pt(x + w / 2., y + h),
        Side::Left => pt(x, y + h / 2.),
    }
}

/// DocumentsPage.tsx `path`: dik açılı, düğümlerin etrafından dolaşan bağlantı yolu (kırık çizgi noktaları).
pub(super) fn route(nodes: &[CanvasNode], a: P, b: P, a_side: Side, b_side: Side, points: &[P]) -> Vec<P> {
    if !points.is_empty() {
        let mut out = vec![a];
        let mut current = a;
        for next in points.iter().copied().chain(std::iter::once(b)) {
            out.push(pt(next.x, current.y));
            out.push(next);
            current = next;
        }
        return out;
    }
    let gap = 18.;
    let offset = |p: P, s: Side| match s {
        Side::Top => pt(p.x, p.y - gap),
        Side::Right => pt(p.x + gap, p.y),
        Side::Bottom => pt(p.x, p.y + gap),
        Side::Left => pt(p.x - gap, p.y),
    };
    let (a_out, b_out) = (offset(a, a_side), offset(b, b_side));
    let rects: Vec<(f32, f32, f32, f32)> = nodes.iter().filter(|n| n.kind != "TEXT").map(|n| (n.x as f32 - 2., (n.x + n.w) as f32 + 2., n.y as f32 - 2., (n.y + n.h) as f32 + 2.)).collect();
    let clear = |first: P, last: P| {
        rects.iter().all(|&(left, right, top, bottom)| {
            if first.x == last.x {
                !(first.x > left && first.x < right && first.y.max(last.y) > top && first.y.min(last.y) < bottom)
            } else {
                !(first.y > top && first.y < bottom && first.x.max(last.x) > left && first.x.min(last.x) < right)
            }
        })
    };
    let compact = |r: Vec<P>| -> Vec<P> {
        let mut out: Vec<P> = Vec::new();
        for (i, p) in r.iter().enumerate() {
            if i == 0 || p.x != r[i - 1].x || p.y != r[i - 1].y {
                out.push(*p);
            }
        }
        out
    };
    let mut lanes_x = vec![a_out.x, b_out.x];
    let mut lanes_y = vec![a_out.y, b_out.y];
    for &(l, r, t, bo) in &rects {
        lanes_x.extend([l - 8., r + 8.]);
        lanes_y.extend([t - 8., bo + 8.]);
    }
    let mut candidates = vec![compact(vec![a, a_out, pt(b_out.x, a_out.y), b_out, b]), compact(vec![a, a_out, pt(a_out.x, b_out.y), b_out, b])];
    for x in lanes_x {
        candidates.push(compact(vec![a, a_out, pt(x, a_out.y), pt(x, b_out.y), b_out, b]));
    }
    for y in lanes_y {
        candidates.push(compact(vec![a, a_out, pt(a_out.x, y), pt(b_out.x, y), b_out, b]));
    }
    let length = |r: &Vec<P>| r.windows(2).map(|w| (w[1].x - w[0].x).abs() + (w[1].y - w[0].y).abs()).sum::<f32>();
    // Electron: route.slice(1,-2).every((item,index)=>clear(item,route[index+2]))
    let valid = |r: &Vec<P>| {
        if r.len() < 3 {
            return true;
        }
        let inner = &r[1..r.len() - 2];
        inner.iter().enumerate().all(|(i, item)| clear(*item, r[i + 2]))
    };
    let mut ok: Vec<&Vec<P>> = candidates.iter().filter(|r| valid(r)).collect();
    // Array.sort kararlıdır: eşit uzunlukta ilk aday kalır.
    ok.sort_by(|x, y| length(x).partial_cmp(&length(y)).unwrap_or(std::cmp::Ordering::Equal));
    ok.first().map(|r| (*r).clone()).unwrap_or_else(|| candidates[0].clone())
}

/// Kırık çizginin uzunluğa göre orta noktası (`wireCenter`).
fn polyline_center(r: &[P]) -> P {
    if r.len() < 2 {
        return r.first().copied().unwrap_or(pt(0., 0.));
    }
    let lengths: Vec<f32> = r.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).collect();
    let half = lengths.iter().sum::<f32>() / 2.;
    let mut walked = 0.;
    for (i, l) in lengths.iter().enumerate() {
        if walked + l >= half {
            let ratio = if *l > 0. { (half - walked) / l } else { 0. };
            return pt(r[i].x + (r[i + 1].x - r[i].x) * ratio, r[i].y + (r[i + 1].y - r[i].y) * ratio);
        }
        walked += l;
    }
    *r.last().unwrap()
}

/// Bağlantının bağlandığı düğümler var mı (düz metne bağlantı yok).
fn wire_ends<'a>(nodes: &'a [CanvasNode], w: &CanvasWire) -> Option<(&'a CanvasNode, &'a CanvasNode)> {
    let a = nodes.iter().find(|n| n.id == w.from)?;
    let b = nodes.iter().find(|n| n.id == w.to)?;
    (a.kind != "TEXT" && b.kind != "TEXT").then_some((a, b))
}

pub(super) fn wire_route(nodes: &[CanvasNode], w: &CanvasWire) -> Option<Vec<P>> {
    let (a, b) = wire_ends(nodes, w)?;
    let (fs_, ts) = (wire_side(w, "fromSide"), wire_side(w, "toSide"));
    Some(route(nodes, side_point(a, fs_), side_point(b, ts), fs_, ts, &wire_points(w)))
}

/// Paletteki sabit renkler (DocumentsPage.tsx `colors`).
const SWATCHES: [&str; 8] = ["#26345f", "#315b8a", "#2f6a5b", "#6b3f7d", "#8a5b31", "#7d3643", "#d8dbe8", "#202431"];

fn uid() -> String {
    format!("n{}-{}", chrono::Utc::now().timestamp_millis(), rand_tail())
}
fn rand_tail() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

// ------------------------------------------------------------------ eylemler

impl DocumentsPage {
    /// Düğüm ekler (Electron `add`): ölçü türe göre, konum görünür alanın ortası ya da verilen nokta.
    pub(super) fn canvas_add(&mut self, kind_: &str, at: Option<P>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return;
        }
        let Some(c) = &mut self.canvas else { return };
        let (w, h) = match kind_ {
            "SPHERE" => (180., 180.),
            "TRIANGLE" => (210., 180.),
            "IMAGE" => (420., 300.),
            "TEXT" => (140., 37.),
            _ => (280., 180.),
        };
        let b = c.bounds.get();
        let center = at.unwrap_or_else(|| pt(c.view.x + f32::from(b.size.width) / c.zoom / 2., c.view.y + f32::from(b.size.height) / c.zoom / 2.));
        c.remember();
        let id = uid();
        let mut extra = Map::new();
        extra.insert("borderWidth".into(), json!(if kind_ == "TEXT" { 0 } else { 2 }));
        extra.insert("borderColor".into(), json!("#000000"));
        extra.insert("fontSize".into(), json!(if kind_ == "TEXT" { 18 } else { 14 }));
        c.content.nodes.push(CanvasNode {
            id: id.clone(),
            kind: kind_.into(),
            x: (center.x - w / 2.) as f64,
            y: (center.y - h / 2.) as f64,
            w: w as f64,
            h: h as f64,
            text: Some(String::new()),
            color: Some(if kind_ == "TEXT" { "transparent".into() } else { "#1d2f57".into() }),
            extra,
        });
        c.selection = vec![id.clone()];
        c.selected_wire = None;
        c.format_suppressed = false;
        self.schedule_save(window, cx);
        if kind_ == "TEXT" {
            self.begin_text(id, window, cx);
        }
    }

    pub(super) fn canvas_cycle_grid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let next = match c.grid_mode().as_str() {
            "DOTS" => "LINES",
            "LINES" => "NONE",
            _ => "DOTS",
        };
        c.remember();
        c.content.extra.insert("grid".into(), json!(next));
        self.schedule_save(window, cx);
    }

    pub(super) fn canvas_remove_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        if let Some(w) = c.selected_wire.take() {
            c.remember();
            c.content.connections.retain(|x| x.id != w);
        } else if !c.selection.is_empty() {
            c.remember();
            let sel = std::mem::take(&mut c.selection);
            c.content.nodes.retain(|n| !sel.contains(&n.id));
            c.content.connections.retain(|w| !sel.contains(&w.from) && !sel.contains(&w.to));
            c.editing = None;
        } else {
            return;
        }
        self.schedule_save(window, cx);
    }

    fn canvas_undo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let next = if redo { c.redo.pop() } else { c.undo.pop() };
        let Some(next) = next else { return };
        let current = std::mem::replace(&mut c.content, next);
        if redo { c.undo.push(current) } else { c.redo.push(current) }
        c.selection.clear();
        c.selected_wire = None;
        self.schedule_save(window, cx);
    }

    fn canvas_copy(&mut self) {
        let Some(c) = &mut self.canvas else { return };
        if c.selection.is_empty() {
            return;
        }
        let nodes: Vec<CanvasNode> = c.content.nodes.iter().filter(|n| c.selection.contains(&n.id)).cloned().collect();
        let wires: Vec<CanvasWire> = c.content.connections.iter().filter(|w| c.selection.contains(&w.from) && c.selection.contains(&w.to)).cloned().collect();
        c.clipboard = Some((nodes, wires));
    }

    pub(super) fn canvas_paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return;
        }
        let Some(c) = &mut self.canvas else { return };
        let Some((nodes, wires)) = c.clipboard.clone().filter(|(n, _)| !n.is_empty()) else { return };
        let min_x = nodes.iter().map(|n| n.x).fold(f64::MAX, f64::min) as f32;
        let min_y = nodes.iter().map(|n| n.y).fold(f64::MAX, f64::min) as f32;
        let max_x = nodes.iter().map(|n| n.x + n.w).fold(f64::MIN, f64::max) as f32;
        let max_y = nodes.iter().map(|n| n.y + n.h).fold(f64::MIN, f64::max) as f32;
        let b = c.bounds.get();
        let fallback = pt(c.view.x + f32::from(b.size.width) / c.zoom / 2., c.view.y + f32::from(b.size.height) / c.zoom / 2.);
        let target = c.cursor.unwrap_or(fallback);
        let (dx, dy) = ((target.x - (min_x + max_x) / 2.) as f64, (target.y - (min_y + max_y) / 2.) as f64);
        let ids: Vec<(String, String)> = nodes.iter().map(|n| (n.id.clone(), uid())).collect();
        let map = |id: &str| ids.iter().find(|(a, _)| a == id).map(|(_, b)| b.clone()).unwrap_or_default();
        let pasted: Vec<CanvasNode> = nodes.iter().map(|n| CanvasNode { id: map(&n.id), x: n.x + dx, y: n.y + dy, ..n.clone() }).collect();
        let pasted_wires: Vec<CanvasWire> = wires.iter().map(|w| CanvasWire { id: uid(), from: map(&w.from), to: map(&w.to), extra: w.extra.clone() }).collect();
        c.remember();
        c.selection = pasted.iter().map(|n| n.id.clone()).collect();
        c.content.nodes.extend(pasted.clone());
        c.content.connections.extend(pasted_wires.clone());
        c.clipboard = Some((pasted, pasted_wires));
        c.selected_wire = None;
        self.schedule_save(window, cx);
    }

    /// Seçili düğümlere yama (biçim paneli).
    fn canvas_patch(&mut self, patch: Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        if c.selection.is_empty() {
            return;
        }
        c.remember();
        for n in c.content.nodes.iter_mut().filter(|n| c.selection.contains(&n.id)) {
            if let Some(obj) = patch.as_object() {
                for (k, v) in obj {
                    match k.as_str() {
                        "color" => n.color = v.as_str().map(str::to_string),
                        _ => {
                            n.extra.insert(k.clone(), v.clone());
                        }
                    }
                }
            }
        }
        self.schedule_save(window, cx);
    }

    fn canvas_patch_wire(&mut self, patch: Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let Some(id) = c.selected_wire.clone() else { return };
        c.remember();
        if let Some(w) = c.content.connections.iter_mut().find(|w| w.id == id) {
            if let Some(obj) = patch.as_object() {
                for (k, v) in obj {
                    w.extra.insert(k.clone(), v.clone());
                }
            }
        }
        self.schedule_save(window, cx);
    }

    fn canvas_emoji(&mut self, id: &str, emoji: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        c.remember();
        if let Some(n) = c.content.nodes.iter_mut().find(|n| n.id == id) {
            let mut list: Vec<Value> = n.extra.get("emojis").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            if list.is_empty() {
                if let Some(e) = n.extra.get("emoji").cloned() {
                    list.push(e);
                }
            }
            n.extra.remove("emoji");
            match emoji {
                Some(e) => list.push(json!(e)),
                None => list.clear(),
            }
            n.extra.insert("emojis".into(), Value::Array(list));
        }
        c.menu = None;
        self.schedule_save(window, cx);
    }

    // -------------------------------------------------------------- olaylar

    fn canvas_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let key = e.keystroke.key.as_str();
        if key == "escape" {
            c.menu = None;
            self.commit_text(window, cx);
            cx.notify();
            return;
        }
        if c.editing.is_some() {
            return;
        }
        let ctrl = e.keystroke.modifiers.control;
        let writable = self.doc.as_ref().is_some_and(|d| d.writable());
        match (ctrl, key) {
            (true, "z") if e.keystroke.modifiers.shift => self.canvas_undo(true, window, cx),
            (true, "z") => self.canvas_undo(false, window, cx),
            (true, "y") => self.canvas_undo(true, window, cx),
            (true, "c") => self.canvas_copy(),
            (true, "v") => self.canvas_paste(window, cx),
            (_, "delete") if writable => self.canvas_remove_selected(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn canvas_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.commit_text(window, cx);
        let Some(c) = &mut self.canvas else { return };
        c.menu = None;
        c.selected_wire = None;
        let at = c.world(e.position);
        let base = if e.modifiers.control { c.selection.clone() } else { Vec::new() };
        if !e.modifiers.control {
            c.selection.clear();
        }
        c.drag = CanvasDrag::Marquee { start: at, base };
        c.marquee = Some((at.x, at.y, 0., 0.));
        cx.notify();
    }

    fn canvas_right_down(&mut self, e: &MouseDownEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        c.drag = CanvasDrag::Pan { from: e.position, view: c.view, moved: false };
    }

    fn canvas_move(&mut self, e: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let at = c.world(e.position);
        c.cursor = Some(at);
        match c.drag.clone() {
            CanvasDrag::Pan { from, view, moved } => {
                let d = e.position - from;
                if !moved && f32::from(d.x).hypot(f32::from(d.y)) < 3. {
                    return;
                }
                c.view = pt(view.x - f32::from(d.x) / c.zoom, view.y - f32::from(d.y) / c.zoom);
                c.drag = CanvasDrag::Pan { from, view, moved: true };
                cx.notify();
            }
            CanvasDrag::Move { start, originals, remembered } => {
                let (dx, dy) = (at.x - start.x, at.y - start.y);
                if dx.abs() < 0.5 && dy.abs() < 0.5 {
                    return;
                }
                if !remembered {
                    c.remember();
                    c.drag = CanvasDrag::Move { start, originals: originals.clone(), remembered: true };
                }
                let (mut mx, mut my) = (dx, dy);
                if e.modifiers.shift {
                    // Shift: ızgaraya ve diğer düğümlere hizala (Electron snap).
                    if let Some((aid, ox, oy)) = originals.first() {
                        if let Some(anchor) = c.content.nodes.iter().find(|n| &n.id == aid).cloned() {
                            let (aw, ah) = (anchor.w as f32, anchor.h as f32);
                            let (px_, py_) = (ox + dx, oy + dy);
                            let gx = ((px_ + aw / 2.) / SNAP).round() * SNAP;
                            let gy = ((py_ + ah / 2.) / SNAP).round() * SNAP;
                            mx += gx - (px_ + aw / 2.);
                            my += gy - (py_ + ah / 2.);
                        }
                    }
                }
                for (id, ox, oy) in &originals {
                    if let Some(n) = c.content.nodes.iter_mut().find(|n| &n.id == id) {
                        n.x = (ox + mx) as f64;
                        n.y = (oy + my) as f64;
                    }
                }
                cx.notify();
            }
            CanvasDrag::Resize { id, dir, start, node } => {
                let (dx, dy) = (f32::from(e.position.x - start.x) / c.zoom, f32::from(e.position.y - start.y) / c.zoom);
                let (nx, ny, nw, nh) = (node.x as f32, node.y as f32, node.w as f32, node.h as f32);
                let (mut x, mut y, mut w, mut h) = (nx, ny, nw, nh);
                if dir.contains('e') {
                    w = nw + dx;
                }
                if dir.contains('s') {
                    h = nh + dy;
                }
                if dir.contains('w') {
                    w = nw - dx;
                    x = nx + dx;
                }
                if dir.contains('n') {
                    h = nh - dy;
                    y = ny + dy;
                }
                match kind(&node) {
                    "SPHERE" => {
                        let s = w.max(h).max(80.);
                        w = s;
                        h = s;
                        if dir.contains('w') {
                            x = nx + nw - s;
                        }
                        if dir.contains('n') {
                            y = ny + nh - s;
                        }
                    }
                    "IMAGE" => {
                        let hor = if dir.contains('e') { dx } else { -dx };
                        let ver = if dir.contains('s') { dy } else { -dy };
                        let scale = (60. / nw).max(60. / nh).max(1. + if (hor / nw).abs() >= (ver / nh).abs() { hor / nw } else { ver / nh });
                        w = nw * scale;
                        h = nh * scale;
                        x = if dir.contains('w') { nx + nw - w } else { nx };
                        y = if dir.contains('n') { ny + nh - h } else { ny };
                    }
                    _ => {
                        w = w.max(100.);
                        h = h.max(70.);
                    }
                }
                if let Some(n) = c.content.nodes.iter_mut().find(|n| n.id == id) {
                    n.x = x as f64;
                    n.y = y as f64;
                    n.w = w as f64;
                    n.h = h as f64;
                }
                cx.notify();
            }
            CanvasDrag::Marquee { start, base } => {
                let r = (start.x.min(at.x), start.y.min(at.y), (at.x - start.x).abs(), (at.y - start.y).abs());
                // Önce sınır kutusu çakışması, sonra gerçek şekil (üçgenin boş köşesi seçmez).
                let hits = c
                    .content
                    .nodes
                    .iter()
                    .filter(|n| (n.x as f32) < r.0 + r.2 && ((n.x + n.w) as f32) > r.0 && (n.y as f32) < r.1 + r.3 && ((n.y + n.h) as f32) > r.1)
                    .filter(|n| polygon_intersects_rect(&shape_outline(kind(n), n.w as f32, n.h as f32, 0.), (r.0 - n.x as f32, r.1 - n.y as f32, r.2, r.3)))
                    .map(|n| n.id.clone());
                let mut sel = base.clone();
                for h in hits {
                    if !sel.contains(&h) {
                        sel.push(h);
                    }
                }
                c.selection = sel;
                c.marquee = Some(r);
                cx.notify();
            }
            CanvasDrag::Wire { from, side, .. } => {
                c.drag = CanvasDrag::Wire { from, side, at };
                cx.notify();
            }
            CanvasDrag::None => {}
        }
    }

    fn canvas_up(&mut self, e: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let drag = std::mem::replace(&mut c.drag, CanvasDrag::None);
        c.marquee = None;
        match drag {
            CanvasDrag::Move { remembered: true, .. } | CanvasDrag::Resize { .. } => {
                crate::trace("doküman: düğüm taşındı");
                self.schedule_save(window, cx);
            }
            CanvasDrag::Pan { moved: false, .. } if e.button == MouseButton::Right => {
                // Kısa sağ tık: hızlı araçlar menüsü (ya da düğüm menüsü).
                let at = c.world(e.position);
                let hit = c.content.nodes.iter().rev().find(|n| node_hit(n, at, 0.)).map(|n| n.id.clone());
                c.menu = Some(match hit {
                    Some(id) => {
                        c.selection = vec![id.clone()];
                        c.selected_wire = None;
                        c.format_suppressed = true;
                        CanvasMenu::Node { id, at: e.position }
                    }
                    None => CanvasMenu::Canvas { at: e.position, world: at },
                });
                cx.notify();
            }
            CanvasDrag::Wire { from, side, at } => {
                // Bırakılan düğüm: dörtte birlik kenar bölgesine göre taraf (Electron `onPointerUp`).
                let target = c.content.nodes.iter().rev().find(|n| n.id != from && n.kind != "TEXT" && node_hit(n, at, 6. / c.zoom)).cloned();
                if let Some(t) = target {
                    let (rx, ry) = (at.x - t.x as f32, at.y - t.y as f32);
                    let to_side = if rx < t.w as f32 * 0.25 {
                        Side::Left
                    } else if rx > t.w as f32 * 0.75 {
                        Side::Right
                    } else if ry < t.h as f32 / 2. {
                        Side::Top
                    } else {
                        Side::Bottom
                    };
                    c.remember();
                    let id = uid();
                    let mut extra = Map::new();
                    extra.insert("fromSide".into(), json!(side.name()));
                    extra.insert("toSide".into(), json!(to_side.name()));
                    c.content.connections.push(CanvasWire { id: id.clone(), from, to: t.id.clone(), extra });
                    c.selected_wire = Some(id);
                    self.schedule_save(window, cx);
                }
                cx.notify();
            }
            _ => cx.notify(),
        }
    }

    fn canvas_wheel(&mut self, e: &ScrollEvent, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let d = e.delta.pixel_delta(px(20.));
        if f32::from(d.y) == 0. {
            return;
        }
        // Electron `wheelZoom`: her tekerlek adımı imleç çevresinde yakınlaştırır (0,2–3).
        let next = ((c.zoom * if f32::from(d.y) > 0. { 1.12 } else { 0.88 }) * 1000.).round() / 1000.;
        let next = next.clamp(0.2, 3.);
        if next == c.zoom {
            return;
        }
        let anchor = c.world(e.position);
        let b = c.bounds.get();
        let (cx_, cy_) = (f32::from(e.position.x - b.origin.x), f32::from(e.position.y - b.origin.y));
        c.zoom = next;
        c.view = pt(anchor.x - cx_ / next, anchor.y - cy_ / next);
        cx.notify();
    }

    fn node_down(&mut self, id: String, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // İsabet testi gerçek şekli izler: üçgenin/kürenin sınır kutusundaki boş köşeye tıklamak düğümü
        // seçmez; olay yayılmaya devam eder (alttaki düğüm ya da tuval seçim kutusu alır).
        let editing_this = self.canvas.as_ref().and_then(|c| c.editing.as_ref()).is_some_and(|(eid, _)| *eid == id);
        if !editing_this {
            if let Some(c) = &self.canvas {
                let at = c.world(e.position);
                if c.node(&id).is_some_and(|n| !node_hit(n, at, 3. / c.zoom)) {
                    return;
                }
            }
        }
        cx.stop_propagation();
        window.focus(&self.focus, cx);
        let writable = self.doc.as_ref().is_some_and(|d| d.writable());
        if e.click_count >= 2 {
            let image = self.canvas.as_ref().and_then(|c| c.node(&id)).is_some_and(|n| kind(n) == "IMAGE");
            if writable && !image {
                self.begin_text(id, window, cx);
            }
            return;
        }
        if self.canvas.as_ref().and_then(|c| c.editing.as_ref()).is_some_and(|(eid, _)| *eid == id) {
            return;
        }
        self.commit_text(window, cx);
        let Some(c) = &mut self.canvas else { return };
        c.menu = None;
        c.selected_wire = None;
        c.format_suppressed = false;
        let ids = if e.modifiers.control {
            if c.selection.contains(&id) { c.selection.clone() } else { c.selection.iter().cloned().chain(std::iter::once(id.clone())).collect() }
        } else if c.selection.contains(&id) {
            c.selection.clone()
        } else {
            vec![id.clone()]
        };
        c.selection = ids.clone();
        if writable {
            let start = c.world(e.position);
            let mut originals: Vec<(String, f32, f32)> = c.content.nodes.iter().filter(|n| ids.contains(&n.id)).map(|n| (n.id.clone(), n.x as f32, n.y as f32)).collect();
            // Hizalamada çapa: tıklanan düğüm.
            if let Some(i) = originals.iter().position(|o| o.0 == id) {
                originals.swap(0, i);
            }
            c.drag = CanvasDrag::Move { start, originals, remembered: false };
        }
        cx.notify();
    }

    pub(super) fn begin_text(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let text = c.node(&id).and_then(|n| n.text.clone()).unwrap_or_default();
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 12).placeholder("Metin yaz…"));
        editor.update(cx, |t, cx| {
            t.set_value(text, window, cx);
            t.focus(window, cx);
        });
        self._subs.push(cx.subscribe_in(&editor, window, |_, _, e: &InputEvent, _, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        }));
        c.editing = Some((id, editor));
        c.selection = c.editing.as_ref().map(|(i, _)| vec![i.clone()]).unwrap_or_default();
        cx.notify();
    }

    pub(super) fn commit_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        if let Some((id, editor)) = c.editing.take() {
            let text = editor.read(cx).value().to_string();
            if let Some(n) = c.content.nodes.iter_mut().find(|n| n.id == id) {
                if n.text.as_deref() != Some(text.as_str()) {
                    n.text = Some(text);
                    self.schedule_save(window, cx);
                }
            }
        }
    }

    // -------------------------------------------------------------- çizim

    pub(super) fn canvas_view(&mut self, writable: bool, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // Düz metin düğümlerinin ölçüsü her çizimde metinden hesaplanır (Electron `textBounds`).
        if let Some(c) = &mut self.canvas {
            let sizes: Vec<(usize, (f32, f32))> = c.content.nodes.iter().enumerate().filter(|(_, n)| n.kind == "TEXT").map(|(i, n)| (i, text_bounds(n, window))).collect();
            for (i, (w, h)) in sizes {
                c.content.nodes[i].w = w as f64;
                c.content.nodes[i].h = h as f64;
            }
        }
        let viewport = window.viewport_size();
        let Some(c) = &mut self.canvas else { return div().into_any_element() };
        // İlk açılış: içeriğin merkezi görünür alanın ortasına (kaydırma alanı = sahne − 48px araç kutusu).
        if !c.centered {
            let sidebar = if self.sidebar_hidden { 0. } else { crate::resize::value(&crate::resize::DOCUMENTS_SIDEBAR) };
            let (w, h) = (f32::from(viewport.width) - 72. - sidebar - 48., f32::from(viewport.height) - 38. - 56.);
            let nodes = &c.content.nodes;
            let (cx0, cy0) = if nodes.is_empty() {
                (0., 0.)
            } else {
                let min_x = nodes.iter().map(|n| n.x).fold(f64::MAX, f64::min) as f32;
                let min_y = nodes.iter().map(|n| n.y).fold(f64::MAX, f64::min) as f32;
                let max_x = nodes.iter().map(|n| n.x + n.w).fold(f64::MIN, f64::max) as f32;
                let max_y = nodes.iter().map(|n| n.y + n.h).fold(f64::MIN, f64::max) as f32;
                ((min_x + max_x) / 2., (min_y + max_y) / 2.)
            };
            c.view = pt(cx0 - w / 2. / c.zoom, cy0 - h / 2. / c.zoom);
            c.bounds.set(Bounds::new(point(px(72. + sidebar), px(94.)), size(px(w), px(h))));
            c.centered = true;
        }
        if std::mem::take(&mut c.pending_menu) {
            let b = c.bounds.get();
            let at = b.origin + point(b.size.width / 2., b.size.height / 2.);
            let world = c.world(at);
            c.menu = Some(CanvasMenu::Canvas { at, world });
        }
        let (zoom, view) = (c.zoom, c.view);
        let nodes = c.content.nodes.clone();
        let sx = move |x: f32| (x - view.x) * zoom;
        let sy = move |y: f32| (y - view.y) * zoom;

        // ---- ızgara + bağlantılar (tek boyama katmanı)
        let grid = c.grid_mode();
        let grid_color = parse_color(c.content.extra.get("gridColor").and_then(|v| v.as_str())).unwrap_or(rgb(0xdfe5f0).into());
        let spacing = c.content.extra.get("gridSpacing").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(40.).clamp(10., 100.);
        let opacity = c.content.extra.get("gridOpacity").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.1).clamp(0.02, 0.4);
        let selected_wire = c.selected_wire.clone();
        let wires: Vec<(String, Vec<P>, Hsla, f32, Option<String>, Vec<P>)> = c
            .content
            .connections
            .iter()
            .filter_map(|w| {
                let r = wire_route(&nodes, w)?;
                let color = parse_color(w.extra.get("color").and_then(|v| v.as_str())).unwrap_or(rgb(0x4f9dff).into());
                let thick = w.extra.get("thickness").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(2.2);
                let label = w.extra.get("label").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(str::to_string);
                Some((w.id.clone(), r, color, thick, label, wire_points(w)))
            })
            .collect();
        let draft: Option<Vec<P>> = match &c.drag {
            CanvasDrag::Wire { from, side, at } => nodes.iter().find(|n| &n.id == from).map(|n| route(&nodes, side_point(n, *side), *at, *side, Side::Left, &[])),
            _ => None,
        };
        let center_stroke: Hsla = rgb(0x8da0e7).into();
        let (bg1, accent_hover) = (p.bg_1, p.accent_hover);
        let wire_data = wires.clone();
        let bounds_cell = c.bounds.clone();
        let layer = canvas(
            move |b, _, _| bounds_cell.set(b),
            move |b: Bounds<Pixels>, _, window, _| {
                let o = b.origin;
                let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
                let mut tint = grid_color;
                tint.a = opacity;
                let step = spacing * zoom;
                match grid.as_str() {
                    "DOTS" if step >= 4. => {
                        // radial-gradient(renk 1px, saydam 1px): her karonun ortasında 1px yarıçaplı nokta.
                        let ox = (-view.x * zoom).rem_euclid(step) + step / 2.;
                        let oy = (-view.y * zoom).rem_euclid(step) + step / 2.;
                        let mut y = oy - step;
                        while y < h + step {
                            let mut x = ox - step;
                            while x < w + step {
                                window.paint_quad(quad(Bounds::new(o + point(px(x - 1.), px(y - 1.)), size(px(2.), px(2.))), Corners::all(px(1.)), tint, Edges::default(), gpui_kit::transparent_black(), BorderStyle::default()));
                                x += step;
                            }
                            y += step;
                        }
                    }
                    "LINES" if step >= 4. => {
                        let mut major = grid_color;
                        major.a = (opacity * 1.8).min(0.65);
                        for (st, col) in [(step, tint), (step * 5., major)] {
                            let ox = (-view.x * zoom).rem_euclid(st);
                            let oy = (-view.y * zoom).rem_euclid(st);
                            let mut x = ox;
                            while x < w {
                                window.paint_quad(gpui_kit::fill(Bounds::new(o + point(px(x), px(0.)), size(px(1.), b.size.height)), col));
                                x += st;
                            }
                            let mut y = oy;
                            while y < h {
                                window.paint_quad(gpui_kit::fill(Bounds::new(o + point(px(0.), px(y)), size(b.size.width, px(1.))), col));
                                y += st;
                            }
                        }
                    }
                    _ => {}
                }
                let to_screen = |q: P| o + point(px(sx(q.x)), px(sy(q.y)));
                let mut paint_wire = |r: &[P], color: Hsla, thick: f32, arrow: bool| {
                    if r.len() < 2 {
                        return;
                    }
                    let mut path = PathBuilder::stroke(px(thick * zoom));
                    path.move_to(to_screen(r[0]));
                    for q in &r[1..] {
                        path.line_to(to_screen(*q));
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color);
                    }
                    if arrow {
                        // marker: viewBox 10, 8×8 kullanıcı birimi, refX 8,5 — uç, yol sonundan 1,2 birim ileride.
                        let (a, e) = (r[r.len() - 2], r[r.len() - 1]);
                        let len = (e.x - a.x).hypot(e.y - a.y).max(0.001);
                        let (ux, uy) = ((e.x - a.x) / len, (e.y - a.y) / len);
                        let tip = pt(e.x + ux * 1.2, e.y + uy * 1.2);
                        let base = pt(e.x - ux * 6.8, e.y - uy * 6.8);
                        let mut head = PathBuilder::fill();
                        head.move_to(to_screen(tip));
                        head.line_to(to_screen(pt(base.x - uy * 4., base.y + ux * 4.)));
                        head.line_to(to_screen(pt(base.x + uy * 4., base.y - ux * 4.)));
                        head.close();
                        if let Ok(path) = head.build() {
                            window.paint_path(path, color);
                        }
                    }
                };
                for (id, r, color, thick, _, _) in &wire_data {
                    let selected = selected_wire.as_deref() == Some(id.as_str());
                    if selected {
                        // Seçili bağlantı: altında yumuşak vurgu.
                        let mut glow = accent_hover;
                        glow.a = 0.35;
                        paint_wire(r, glow, thick + 4., false);
                    }
                    paint_wire(r, *color, *thick, true);
                }
                if let Some(r) = &draft {
                    let mut col = accent_hover;
                    col.a = 0.9;
                    paint_wire(r, col, 2., false);
                }
                // Orta nokta halkaları (r 5, dolgu bg-1, çizgi 2px).
                for (id, r, _, _, _, points) in &wire_data {
                    let m = polyline_center(r);
                    let c = to_screen(m);
                    let rad = 6. * zoom;
                    let sel = selected_wire.as_deref() == Some(id.as_str());
                    window.paint_quad(quad(Bounds::new(c - point(px(rad), px(rad)), size(px(rad * 2.), px(rad * 2.))), Corners::all(px(rad)), if sel { accent_hover } else { bg1 }, Edges::all(px(2. * zoom)), if sel { gpui_kit::white() } else { center_stroke }, BorderStyle::default()));
                    if sel {
                        for q in points {
                            let c = to_screen(*q);
                            let rad = 8. * zoom;
                            window.paint_quad(quad(Bounds::new(c - point(px(rad), px(rad)), size(px(rad * 2.), px(rad * 2.))), Corners::all(px(rad)), accent_hover, Edges::all(px(2. * zoom)), bg1, BorderStyle::default()));
                        }
                    }
                }
            },
        )
        .absolute()
        .inset_0();

        // ---- bağlantı etiketleri (svg text: 600 10,2px, bg-0 renginde 5px kontur hale)
        let mut labels = Vec::new();
        for (id, r, _, _, label, _) in &wires {
            let Some(label) = label else { continue };
            let m = polyline_center(r);
            let fs_ = fs(12.) * zoom;
            let lh = 13. * zoom;
            let base = |dx: f32, dy: f32, color: Hsla| {
                div()
                    .absolute()
                    .left(px(sx(m.x) - 150. + dx))
                    .w(px(300.))
                    .top(px(sy(m.y - 11.) - f32::from(fs_) * 0.95 + dy))
                    .h(px(lh))
                    .flex()
                    .justify_center()
                    .text_size(fs_)
                    .line_height(px(lh))
                    .font_family("Arial")
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(label.clone())
            };
            let mut halo = div().absolute().inset_0();
            for (dx, dy) in [(-2., 0.), (2., 0.), (0., -2.), (0., 2.), (-1.6, -1.6), (1.6, -1.6), (-1.6, 1.6), (1.6, 1.6), (-2.4, 0.), (2.4, 0.)] {
                halo = halo.child(base(dx, dy, p.bg_0));
            }
            let _ = id;
            labels.push(halo.child(base(0., 0., p.text_strong)).into_any_element());
        }

        // ---- düğümler
        let selection = c.selection.clone();
        let editing = c.editing.clone();
        let single = selection.len() == 1;
        let mut node_els = Vec::new();
        for n in &nodes {
            let k = kind(n).to_string();
            let (x, y, w, h) = (sx(n.x as f32), sy(n.y as f32), n.w as f32 * zoom, n.h as f32 * zoom);
            let bw = num(n, "borderWidth").unwrap_or(2.);
            let border = css_color(text_of(n, "borderColor").as_deref(), gpui_kit::black());
            let fill = css_color(n.color.as_deref(), rgb(0x26345f).into());
            let selected = selection.contains(&n.id);
            let (id, id2) = (n.id.clone(), n.id.clone());
            let font_size = num(n, "fontSize").unwrap_or(14.);
            let align = text_of(n, "textAlign").unwrap_or_else(|| "center".into());
            let valign = text_of(n, "verticalAlign").unwrap_or_else(|| "middle".into());
            let shadow = vec![gpui_kit::BoxShadow { color: gpui_kit::hsla(0., 0., 0., 0.28), offset: point(px(0.), px(8. * zoom)), blur_radius: px(22. * zoom), spread_radius: px(0.), inset: false }];
            let mut el = div().id(SharedString::from(format!("node-{}", n.id))).absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).group(SharedString::from(format!("node-{}", n.id)));
            // Kenarlık: düğümün dışına taşan `::after` (inset −bw).
            if bw > 0. && k != "TEXT" {
                let b = bw * zoom;
                el = match k.as_str() {
                    "TRIANGLE" => el.child(
                        canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
                            let mut tri = PathBuilder::fill();
                            tri.move_to(bb.origin + point(bb.size.width / 2., px(0.)));
                            tri.line_to(bb.origin + point(bb.size.width, bb.size.height));
                            tri.line_to(bb.origin + point(px(0.), bb.size.height));
                            tri.close();
                            if let Ok(path) = tri.build() {
                                window.paint_path(path, border);
                            }
                        })
                        .absolute()
                        .left(px(-b))
                        .top(px(-b))
                        .w(px(w + 2. * b))
                        .h(px(h + 2. * b)),
                    ),
                    _ => el,
                };
            }
            // Gövde
            let body = match k.as_str() {
                "TRIANGLE" => div().absolute().inset_0().child(
                    canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
                        let mut tri = PathBuilder::fill();
                        tri.move_to(bb.origin + point(bb.size.width / 2., px(0.)));
                        tri.line_to(bb.origin + point(bb.size.width, bb.size.height));
                        tri.line_to(bb.origin + point(px(0.), bb.size.height));
                        tri.close();
                        if let Ok(path) = tri.build() {
                            window.paint_path(path, fill);
                        }
                    })
                    .absolute()
                    .inset_0(),
                ),
                "SPHERE" => div().absolute().inset_0().rounded_full().bg(fill).shadow(shadow.clone()),
                "TEXT" => div().absolute().inset_0(),
                "IMAGE" => div().absolute().inset_0().rounded(px(4. * zoom)).bg(p.bg_2).border_1().border_color(p.border_strong),
                _ => div().absolute().inset_0().bg(fill).shadow(shadow.clone()),
            };
            el = el.child(body);
            if bw > 0. && k != "TEXT" && k != "TRIANGLE" {
                let b = bw * zoom;
                el = el.child(div().absolute().left(px(-b)).top(px(-b)).w(px(w + 2. * b)).h(px(h + 2. * b)).border(px(b)).border_color(border).when(k == "SPHERE", |s| s.rounded_full()));
            }
            // Metin
            let (pt_, pr, pb, pl) = match k.as_str() {
                "TRIANGLE" => (28., 38., 14., 38.),
                "TEXT" => (5., 6., 5., 6.),
                _ => (12., 18., 12., 18.),
            };
            let line_h = if k == "TEXT" { 20.955 } else { 17.255 };
            let mut text_box = div()
                .w_full()
                .pt(px(pt_ * zoom))
                .pr(px(pr * zoom))
                .pb(px(pb * zoom))
                .pl(px(pl * zoom))
                .when(k != "TEXT", |s| s.min_h(px(42. * zoom)))
                .text_size(px(font_size * zoom))
                .line_height(px(line_h * zoom))
                .font_weight(if flag(n, "bold") { FontWeight::EXTRA_BOLD } else { FontWeight::MEDIUM })
                .when(flag(n, "italic"), |s| s.italic())
                .when_some(text_of(n, "fontFamily").filter(|f| f != "inherit"), |s, f| s.font_family(SharedString::from(f.trim_matches('\'').to_string())))
                .text_color(rgb(0xeef1ff));
            text_box = match align.as_str() {
                "left" => text_box.text_left(),
                "right" => text_box.text_right(),
                _ => text_box.text_center(),
            };
            let editor = editing.as_ref().filter(|(eid, _)| *eid == n.id).map(|(_, e)| e.clone());
            if k == "IMAGE" {
                let name = text_of(n, "fileName");
                el = el.child(div().absolute().inset_0().flex().items_center().justify_center().text_size(fs(11.) * zoom).text_color(p.text_3).child(match name {
                    Some(f) => format!("▤ {f}"),
                    None => "Dosya veya medya".into(),
                }));
            } else {
                let content: AnyElement = match editor {
                    Some(e) => Textarea::new(&e).appearance(false).text_size(px(font_size * zoom)).text_color(rgb(0xeef1ff)).into_any_element(),
                    None => div().child(n.text.clone().unwrap_or_default()).into_any_element(),
                };
                let mut wrap = div().absolute().inset_0().flex().flex_col();
                wrap = match valign.as_str() {
                    "top" => wrap.justify_start(),
                    "bottom" => wrap.justify_end(),
                    _ => wrap.justify_center(),
                };
                el = el.child(wrap.child(text_box.child(content)));
            }
            // Emoji rozetleri (sağ üst).
            let emojis: Vec<String> = n.extra.get("emojis").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_string)).collect()).filter(|v: &Vec<String>| !v.is_empty()).or_else(|| text_of(n, "emoji").map(|e| vec![e])).unwrap_or_default();
            if !emojis.is_empty() {
                let mut row = div().absolute().right(px(-9.)).top(px(-12.)).flex().items_center();
                for (i, e) in emojis.iter().enumerate() {
                    row = row.child(div().size(px(24.)).when(i > 0, |s| s.ml(px(-5.))).flex().items_center().justify_center().rounded_full().border_1().border_color(p.border_strong).bg(p.bg_2).shadow(vec![gpui_kit::BoxShadow { color: gpui_kit::hsla(0., 0., 0., 0.35), offset: point(px(0.), px(4.)), blur_radius: px(12.), spread_radius: px(0.), inset: false }]).text_size(fs(15.)).font_family(p2_ui::tokens::FONT_EMOJI).child(e.clone()));
                }
                el = el.child(row);
            }
            // Seçim vurgusu: Electron'daki `::before` kutusu yerine (kullanıcı isteğiyle bilinçli sapma) şeklin
            // kendi dış hattını izler — üçgende üç kenar, kürede elips. Kenarlığın 3px dışında, nötr gri
            // 2px çizgi + altında 4px zıt tonlu ince çizgi (mavi şekillerde ve iki temada da seçilir).
            if selected {
                let (main, under) = selection_colors(p);
                let grow = 3. + if bw > 0. && k != "TEXT" { bw * zoom } else { 0. };
                let outline = shape_outline(&k, w, h, grow);
                el = el.child(
                    canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
                        stroke_polygon(window, bb.origin, &outline, 4., under);
                        stroke_polygon(window, bb.origin, &outline, 2., main);
                    })
                    .absolute()
                    .inset_0(),
                );
            }
            // Bağlantı noktaları (13px, opaklık .2; üzerine gelince / seçiliyken 1).
            if k != "TEXT" {
                // .documents-port: bg-selected zemin, blue-200 kenar; üzerine gelince accent-hover + beyaz.
                let (port_bg, port_border, port_hover) = (p.bg_selected, p.blue_200, p.accent_hover);
                let gid = SharedString::from(format!("node-{}", n.id));
                for side in Side::ALL {
                    let (cxp, cyp) = match side {
                        Side::Top => (w / 2., -0.5),
                        Side::Right => (w + 0.5, h / 2.),
                        Side::Bottom => (w / 2., h + 0.5),
                        Side::Left => (-0.5, h / 2.),
                    };
                    let nid = n.id.clone();
                    el = el.child(
                        div()
                            .id(SharedString::from(format!("port-{}-{}", n.id, side.name())))
                            .absolute()
                            .left(px(cxp - 6.5))
                            .top(px(cyp - 6.5))
                            .size(px(13.))
                            .rounded_full()
                            .bg(port_bg)
                            .border_2()
                            .border_color(port_border)
                            .hover(move |s| s.bg(port_hover).border_color(gpui_kit::white()))
                            .opacity(if selected { 1. } else { 0.2 })
                            .group_hover(gid.clone(), |s| s.opacity(1.))
                            .cursor_crosshair()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    if !this.doc.as_ref().is_some_and(|d| d.writable()) {
                                        return;
                                    }
                                    let Some(c) = &mut this.canvas else { return };
                                    if e.modifiers.alt {
                                        // Alt + tık: bu noktadaki bağlantıları sil.
                                        c.remember();
                                        let s = side.name();
                                        c.content.connections.retain(|w| !(w.from == nid && w.extra.get("fromSide").and_then(|v| v.as_str()) == Some(s)) && !(w.to == nid && w.extra.get("toSide").and_then(|v| v.as_str()) == Some(s)));
                                        cx.notify();
                                        return;
                                    }
                                    let at = c.node(&nid).map(|n| side_point(n, side)).unwrap_or(pt(0., 0.));
                                    c.drag = CanvasDrag::Wire { from: nid.clone(), side, at };
                                    c.selection.clear();
                                    c.selected_wire = None;
                                    c.menu = None;
                                    c.format_suppressed = true;
                                    cx.notify();
                                }),
                            ),
                    );
                }
            }
            // Boyutlandırma tutamaçları (tek seçimde).
            if k != "TEXT" && single && selected && writable {
                for dir in ["nw", "ne", "se", "sw"] {
                    let (lx, ly) = match dir {
                        "nw" => (-6., -6.),
                        "ne" => (w - 5., -6.),
                        "se" => (w - 5., h - 5.),
                        _ => (-6., h - 5.),
                    };
                    let nid = n.id.clone();
                    el = el.child(
                        div()
                            .id(SharedString::from(format!("rs-{}-{dir}", n.id)))
                            .absolute()
                            .left(px(lx))
                            .top(px(ly))
                            .size(px(11.))
                            .rounded(px(2.))
                            .border_2()
                            .border_color(selection_colors(p).1)
                            .bg(selection_colors(p).0)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    let Some(c) = &mut this.canvas else { return };
                                    let Some(node) = c.node(&nid).cloned() else { return };
                                    c.remember();
                                    c.drag = CanvasDrag::Resize { id: nid.clone(), dir, start: e.position, node };
                                    cx.notify();
                                }),
                            ),
                    );
                }
            }
            el = el.on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| this.node_down(id.clone(), e, window, cx)));
            let _ = id2;
            node_els.push(el.into_any_element());
        }

        // ---- seçim kutusu
        // Seçim kutusu da düğüm vurgusuyla aynı nötr gri: ana çizgi + dışında 1px zıt tonlu alt çizgi, hafif dolgu.
        let (sel_main, sel_under) = selection_colors(p);
        let marquee = c.marquee.map(|(mx, my, mw, mh)| {
            let mut fill = sel_main;
            fill.a = 0.08;
            // GPUI gölgesi kutunun altını da boyadığından alt çizgi ayrı bir dış çerçeveyle verilir.
            div()
                .absolute()
                .left(px(sx(mx) - 1.))
                .top(px(sy(my) - 1.))
                .w(px(mw * zoom + 2.))
                .h(px(mh * zoom + 2.))
                .border_1()
                .border_color(sel_under)
                .child(div().absolute().inset_0().border_1().border_color(sel_main).bg(fill))
                .into_any_element()
        });

        // ---- biçim paneli (seçili düğümün yanında) ve bağlantı paneli
        let b = c.bounds.get();
        let (vw, _vh) = (f32::from(b.size.width) / zoom, f32::from(b.size.height) / zoom);
        let format = (!c.format_suppressed && writable)
            .then(|| c.selection.first().and_then(|id| nodes.iter().find(|n| &n.id == id)).cloned())
            .flatten()
            .filter(|_| c.editing.is_none() && matches!(c.drag, CanvasDrag::None))
            .map(|n| {
                let right = (n.x + n.w) as f32 + 14.;
                let left_c = n.x as f32 - 204.;
                let left = if right + 190. <= view.x + vw || left_c < view.x { right } else { left_c };
                (sx(left), sy(n.y as f32), n)
            });
        let format_panel = format.map(|(fx, fy, n)| self.format_panel(&n, fx, fy, p, cx));
        let Some(c) = &self.canvas else { return div().into_any_element() };
        let wire_panel = c.selected_wire.clone().and_then(|id| c.content.connections.iter().find(|w| w.id == id).cloned()).filter(|_| writable).map(|w| self.wire_panel(&w, p, cx));

        let empty = nodes.is_empty().then(|| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .pb(px(f32::from(b.size.height) * 0.08))
                .gap(px(6.))
                .text_color(p.text_3)
                .text_center()
                .child(div().font_weight(FontWeight::BOLD).text_color(rgb(0xa6b0ce)).child("Boş bir tuval"))
                .child(div().text_size(fs(10.)).child("Sağdaki araç kutusundan bir şekil ekle. Sağ tuşla tutup tuvalde gez."))
                .into_any_element()
        });

        let minimap = self.minimap(&nodes, p, cx);
        let scroll = div()
            .id("doc-canvas")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(p.bg_0)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| this.canvas_key(e, window, cx)))
            .child(layer)
            .children(labels)
            .children(node_els)
            .children(marquee)
            .children(format_panel)
            .children(wire_panel)
            .children(empty)
            .child(minimap)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| this.canvas_down(e, window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, window, cx| this.canvas_right_down(e, window, cx)))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, window, cx| this.canvas_move(e, window, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, e: &MouseUpEvent, window, cx| this.canvas_up(e, window, cx)))
            .on_mouse_up(MouseButton::Right, cx.listener(|this, e: &MouseUpEvent, window, cx| this.canvas_up(e, window, cx)))
            .on_scroll(cx.listener(|this, e: &ScrollEvent, _, cx| this.canvas_wheel(e, cx)));
        div().flex_1().min_h_0().w_full().flex().child(scroll).child(self.toolbox(writable, p, cx)).into_any_element()
    }

    /// `.documents-minimap`: 168×108, sağ alt köşe (12px), görünür alan dikdörtgeni; tıklayınca oraya gider.
    fn minimap(&self, nodes: &[CanvasNode], p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(c) = &self.canvas else { return div().into_any_element() };
        let b = c.bounds.get();
        let (vw, vh) = (f32::from(b.size.width) / c.zoom, f32::from(b.size.height) / c.zoom);
        let (mini_w, mini_h) = (168., 108.);
        let (raw_min_x, raw_min_y, raw_max_x, raw_max_y) = if nodes.is_empty() {
            (0., 0., 0., 0.)
        } else {
            (
                nodes.iter().map(|n| n.x).fold(f64::MAX, f64::min) as f32,
                nodes.iter().map(|n| n.y).fold(f64::MAX, f64::min) as f32,
                nodes.iter().map(|n| n.x + n.w).fold(f64::MIN, f64::max) as f32,
                nodes.iter().map(|n| n.y + n.h).fold(f64::MIN, f64::max) as f32,
            )
        };
        let span_x = (raw_max_x - raw_min_x + 800.).max(2400.);
        let span_y = (raw_max_y - raw_min_y + 600.).max(1500.);
        let (vcx, vcy) = (c.view.x + vw / 2., c.view.y + vh / 2.);
        let (min_x, min_y) = (vcx - span_x / 2., vcy - span_y / 2.);
        let scale = (mini_w / span_x).min(mini_h / span_y);
        let (off_x, off_y) = ((mini_w - span_x * scale) / 2., (mini_h - span_y * scale) / 2.);
        let mp = move |x: f32, y: f32| (off_x + (x - min_x) * scale, off_y + (y - min_y) * scale);
        let rects: Vec<(f32, f32, f32, f32, f32, bool)> = nodes
            .iter()
            .map(|n| {
                let (x, y) = mp(n.x as f32, n.y as f32);
                let (w, h) = ((n.w as f32 * scale).max(3.), (n.h as f32 * scale).max(3.));
                let r = if kind(n) == "SPHERE" { (n.w as f32 * scale / 2.).max(2.) } else { 1. };
                (x, y, w, h, r, c.selection.contains(&n.id))
            })
            .collect();
        let (vx, vy) = mp(c.view.x, c.view.y);
        let (vw2, vh2) = ((vw * scale).max(8.), (vh * scale).max(6.));
        let (fill, stroke, accent, soft, accent_soft) = (p.text_3, p.text, p.accent_hover, p.accent_soft, p.accent_hover);
        let _ = accent_soft;
        let map = canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
            let o = bb.origin;
            for (x, y, w, h, r, sel) in &rects {
                window.paint_quad(quad(Bounds::new(o + point(px(*x), px(*y)), size(px(*w), px(*h))), Corners::all(px(*r)), if *sel { accent } else { fill }, Edges::all(px(0.7)), stroke, BorderStyle::default()));
            }
            window.paint_quad(quad(Bounds::new(o + point(px(vx), px(vy)), size(px(vw2), px(vh2))), Corners::all(px(0.)), soft, Edges::all(px(1.25)), accent, BorderStyle::default()));
        })
        .size_full();
        let mut bg = p.bg_1;
        bg.a = 0.94;
        div()
            .id("doc-minimap")
            .absolute()
            .right(px(12.))
            .bottom(px(12.))
            .w(px(170.))
            .h(px(110.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.border_strong)
            .bg(bg)
            .shadow(vec![
                gpui_kit::BoxShadow { color: gpui_kit::hsla(0., 0., 0., 0.45), offset: point(px(0.), px(12.)), blur_radius: px(30.), spread_radius: px(0.), inset: false },
                gpui_kit::BoxShadow { color: gpui_kit::hsla(0., 0., 0., 0.3), offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false },
            ])
            .overflow_hidden()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let Some(c) = &mut this.canvas else { return };
                    let cb = c.bounds.get();
                    // Mini haritanın iç sol üstü: kaydırma alanının sağ altından (12 + 1 kenarlık).
                    let left = cb.origin.x + cb.size.width - px(12. + 169.);
                    let top = cb.origin.y + cb.size.height - px(12. + 109.);
                    let x = min_x + (f32::from(e.position.x - left) - off_x) / scale;
                    let y = min_y + (f32::from(e.position.y - top) - off_y) / scale;
                    c.view = pt(x - vw / 2., y - vh / 2.);
                    cx.notify();
                }),
            )
            .child(map)
            .into_any_element()
    }

    /// `.documents-toolbox`: sağda 48px sütun, 32px araç düğmeleri, ayraç, ızgara düğmesi.
    fn toolbox(&self, writable: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let grid = self.canvas.as_ref().map(|c| c.grid_mode()).unwrap_or_else(|| "DOTS".into());
        let (hbg, hbd, text2, text3) = (p.bg_selected, p.accent_hover, p.text_2, p.text_3);
        let btn = |id: &'static str, title: &'static str, icon: AnyElement, active: bool| {
            div()
                .id(id)
                .size(px(32.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(if active { p.accent_hover } else { p.border })
                .bg(if active { p.accent_soft } else { p.bg_3 })
                .when(writable || active, move |s| s.cursor_pointer().hover(move |s| s.border_color(hbd).bg(hbg)))
                .when(!writable && !active, |s| s.opacity(0.45))
                .tooltip(move |window, cx| p2_ui::controls::tooltip(title, window, cx))
                .child(icon)
        };
        let shape = |round: bool| div().size(px(16.)).border_1().border_color(text2).when(round, |s| s.rounded_full()).into_any_element();
        let tri = canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
            let mut t = PathBuilder::fill();
            t.move_to(bb.origin + point(px(8.5), px(0.)));
            t.line_to(bb.origin + point(px(17.), px(16.)));
            t.line_to(bb.origin + point(px(0.), px(16.)));
            t.close();
            if let Ok(path) = t.build() {
                window.paint_path(path, text2);
            }
        })
        .w(px(17.))
        .h(px(16.))
        .into_any_element();
        let t_icon = div().size(px(18.)).flex().items_center().justify_center().text_size(fs(17.)).font_weight(FontWeight::BOLD).line_height(px(18.)).text_color(text2).child("T").into_any_element();
        let image = line_icon("0 0 24 24", r#"<rect x="3.5" y="4" width="17" height="16" rx="2"/><circle cx="9" cy="9" r="1.5"/><path d="m5.5 17 4.2-4.6 3.1 3 2.4-2.7 3.3 4.3"/>"#, 19., 1.7, true, text3).into_any_element();
        let grid_icon = tool_grid_icon(&grid, p).into_any_element();
        let grid_title = match grid.as_str() {
            "DOTS" => "Grid: Noktalı",
            "LINES" => "Grid: Blueprint",
            _ => "Grid: Kapalı",
        };
        div()
            .w(px(48.))
            .flex_shrink_0()
            .h_full()
            .bg(p.bg_1)
            .border_l_1()
            .border_color(p.border_subtle)
            .child(
                div()
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(5.))
                    .child(btn("tool-box", "Kutu", shape(false), false).on_click(cx.listener(|this, _, window, cx| this.canvas_add("BOX", None, window, cx))))
                    .child(btn("tool-sphere", "Küre", shape(true), false).on_click(cx.listener(|this, _, window, cx| this.canvas_add("SPHERE", None, window, cx))))
                    .child(btn("tool-tri", "Üçgen", tri, false).on_click(cx.listener(|this, _, window, cx| this.canvas_add("TRIANGLE", None, window, cx))))
                    .child(btn("tool-text", "Düz metin", t_icon, false).on_click(cx.listener(|this, _, window, cx| this.canvas_add("TEXT", None, window, cx))))
                    .child(btn("tool-file", "Dosya veya medya", image, false).on_click(cx.listener(|this, _, window, cx| this.canvas_pick_file(window, cx))))
                    .child(div().w(px(26.)).h(px(1.)).my(px(3.)).bg(p.border_subtle))
                    .child(btn("tool-grid", grid_title, grid_icon, grid != "NONE").on_click(cx.listener(|this, _, window, cx| this.canvas_cycle_grid(window, cx)))),
            )
            .into_any_element()
    }

    /// Dosya / medya düğümü: sistem dosya penceresi, yükle, IMAGE düğümü ekle.
    fn canvas_pick_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return;
        }
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Dosya veya medya".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| this.canvas_upload(path, window, cx));
                }
            }
        })
        .detach();
    }

    fn canvas_upload(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(
            window,
            cx,
            move || {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "dosya".into());
                let data = std::fs::read(&path).map_err(|e| ApiError { message: format!("{name} okunamadı: {e}"), ..Default::default() })?;
                api.upload_file(&name, &data)
            },
            |this, result, window, cx| match result {
                Ok(up) => {
                    let mime = up.mime.clone();
                    let (w, h) = if mime.starts_with("video/") {
                        (480., 270.)
                    } else if mime.starts_with("audio/") {
                        (380., 110.)
                    } else if mime.starts_with("image/") {
                        (420., 300.)
                    } else {
                        (360., 120.)
                    };
                    let Some(c) = &mut this.canvas else { return };
                    let b = c.bounds.get();
                    let center = pt(c.view.x + f32::from(b.size.width) / c.zoom / 2., c.view.y + f32::from(b.size.height) / c.zoom / 2.);
                    c.remember();
                    let id = uid();
                    let mut extra = Map::new();
                    extra.insert("url".into(), json!(up.url));
                    extra.insert("mime".into(), json!(mime));
                    extra.insert("fileName".into(), json!(up.name));
                    extra.insert("fileSize".into(), json!(up.size));
                    c.content.nodes.push(CanvasNode { id: id.clone(), kind: "IMAGE".into(), x: (center.x - w / 2.) as f64, y: (center.y - h / 2.) as f64, w: w as f64, h: h as f64, text: None, color: None, extra });
                    c.selection = vec![id];
                    this.schedule_save(window, cx);
                }
                Err(e) => crate::toasts::push(p2_ui::controls::ToastTone::Error, None, if e.message.is_empty() { "Dosya yüklenemedi." } else { &e.message }),
            },
        );
    }

    /// `.documents-format-panel.documents-node-format`: kalın/italik/hizalama, dikey hizalama, renk ızgarası, kenarlık.
    fn format_panel(&self, n: &CanvasNode, left: f32, top: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hb, hbg, hs) = (p.border_accent, p.accent_soft, p.text_strong);
        let fbtn = |id: String, label: AnyElement, active: bool| {
            div()
                .id(SharedString::from(id))
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_color(if active { p.border_accent } else { p.border })
                .bg(if active { p.accent_soft } else { p.bg_3 })
                .text_color(if active { p.text_strong } else { p.text })
                .text_size(fs(11.))
                .cursor_pointer()
                .hover(move |s| s.border_color(hb).bg(hbg).text_color(hs))
                .child(label)
        };
        let align = text_of(n, "textAlign").unwrap_or_else(|| "center".into());
        let valign = text_of(n, "verticalAlign").unwrap_or_else(|| "middle".into());
        let image = kind(n) == "IMAGE";
        let mut panel = div()
            .id("doc-format")
            .w(px(190.))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .rounded(radius::MD)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("doc-format", Entrance::ScaleIn, Duration::ZERO)
            .child(div().text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child("METİN VE ŞEKİL"));
        if !image {
            let (bold, italic) = (flag(n, "bold"), flag(n, "italic"));
            panel = panel
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(fbtn("fmt-b".into(), div().font_weight(FontWeight::EXTRA_BOLD).child("B").into_any_element(), bold).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "bold": !bold }), window, cx))))
                        .child(fbtn("fmt-i".into(), div().italic().child("I").into_any_element(), italic).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "italic": !italic }), window, cx))))
                        .child(fbtn("fmt-l".into(), div().child("≡").into_any_element(), align == "left").on_click(cx.listener(|this, _, window, cx| this.canvas_patch(json!({ "textAlign": "left" }), window, cx))))
                        .child(fbtn("fmt-c".into(), div().child("≣").into_any_element(), align == "center").on_click(cx.listener(|this, _, window, cx| this.canvas_patch(json!({ "textAlign": "center" }), window, cx))))
                        .child(fbtn("fmt-r".into(), div().child("≡").into_any_element(), align == "right").on_click(cx.listener(|this, _, window, cx| this.canvas_patch(json!({ "textAlign": "right" }), window, cx)))),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(fbtn("fmt-vt".into(), div().child("↥").into_any_element(), valign == "top").on_click(cx.listener(|this, _, window, cx| this.canvas_patch(json!({ "verticalAlign": "top" }), window, cx))))
                        .child(fbtn("fmt-vm".into(), div().child("↕").into_any_element(), valign == "middle").on_click(cx.listener(|this, _, window, cx| this.canvas_patch(json!({ "verticalAlign": "middle" }), window, cx))))
                        .child(fbtn("fmt-vb".into(), div().child("↧").into_any_element(), valign == "bottom").on_click(cx.listener(|this, _, window, cx| this.canvas_patch(json!({ "verticalAlign": "bottom" }), window, cx)))),
                );
            let size_now = num(n, "fontSize").unwrap_or(14.);
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(fs(9.))
                    .text_color(p.text_3)
                    .child("Boyut")
                    .child(fbtn("fmt-fs-".into(), div().child("−").into_any_element(), false).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "fontSize": (size_now - 1.).max(8.) }), window, cx))))
                    .child(div().w(px(26.)).text_center().text_color(p.text).child(format!("{size_now:.0}")))
                    .child(fbtn("fmt-fs+".into(), div().child("+").into_any_element(), false).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "fontSize": (size_now + 1.).min(96.) }), window, cx)))),
            );
            let current = n.color.clone().unwrap_or_else(|| "#26345f".into());
            let mut grid = div().flex().flex_wrap().gap(px(5.));
            for col in SWATCHES {
                let active = current.eq_ignore_ascii_case(col);
                let ring = p.accent_hover;
                grid = grid.child(
                    div()
                        .id(SharedString::from(format!("sw-{col}")))
                        .size(px(20.))
                        .rounded_full()
                        .border_2()
                        .border_color(p.bg_2)
                        .bg(css_color(Some(col), gpui_kit::black()))
                        .shadow(vec![gpui_kit::BoxShadow { color: if active { ring } else { p.border }, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(if active { 2. } else { 1. }), inset: false }])
                        .cursor_pointer()
                        .hover(move |s| s.shadow(vec![gpui_kit::BoxShadow { color: ring, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(2.), inset: false }]))
                        .on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "color": col }), window, cx))),
                );
            }
            panel = panel.child(grid);
        }
        let bw = num(n, "borderWidth").unwrap_or(2.);
        panel = panel.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(fs(9.))
                .text_color(p.text_3)
                .child(div().flex_1().child("Kenarlık"))
                .child(fbtn("fmt-border".into(), div().px(px(6.)).child(if bw > 0. { "Açık" } else { "Kapalı" }).into_any_element(), bw > 0.).w(px(56.)).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "borderWidth": if bw > 0. { 0 } else { 2 } }), window, cx))))
                .child(fbtn("fmt-bw-".into(), div().child("−").into_any_element(), false).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "borderWidth": (bw - 1.).max(1.) }), window, cx))))
                .child(fbtn("fmt-bw+".into(), div().child("+").into_any_element(), false).on_click(cx.listener(move |this, _, window, cx| this.canvas_patch(json!({ "borderWidth": (bw + 1.).min(12.) }), window, cx)))),
        );
        div().absolute().left(px(left)).top(px(top)).child(panel).into_any_element()
    }

    /// `.documents-wire-format`: bağlantı rengi, kalınlık, orta etiket (araç kutusunun solunda).
    fn wire_panel(&self, w: &CanvasWire, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let thick = w.extra.get("thickness").and_then(|v| v.as_f64()).unwrap_or(2.2);
        let mut grid = div().flex().flex_wrap().gap(px(5.));
        for col in SWATCHES {
            let ring = p.accent_hover;
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("wsw-{col}")))
                    .size(px(20.))
                    .rounded_full()
                    .border_2()
                    .border_color(p.bg_2)
                    .bg(css_color(Some(col), gpui_kit::black()))
                    .shadow(vec![gpui_kit::BoxShadow { color: p.border, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false }])
                    .cursor_pointer()
                    .hover(move |s| s.shadow(vec![gpui_kit::BoxShadow { color: ring, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(2.), inset: false }]))
                    .on_click(cx.listener(move |this, _, window, cx| this.canvas_patch_wire(json!({ "color": col }), window, cx))),
            );
        }
        let (hb, hbg) = (p.border_accent, p.accent_soft);
        let step = |id: &'static str, label: &'static str, v: f64| {
            div()
                .id(id)
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .cursor_pointer()
                .hover(move |s| s.border_color(hb).bg(hbg))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| this.canvas_patch_wire(json!({ "thickness": v }), window, cx)))
        };
        div()
            .id("doc-wire-format")
            .absolute()
            .right(px(8.))
            .top(px(8.))
            .w(px(190.))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .rounded(radius::MD)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child("WİRE AYARLARI"))
            .child(grid)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(fs(9.))
                    .text_color(p.text_3)
                    .child(div().flex_1().child("Kalınlık"))
                    .child(step("wt-", "−", (thick - 0.5).max(1.)))
                    .child(div().w(px(30.)).text_center().text_color(p.text).child(format!("{thick}px")))
                    .child(step("wt+", "+", (thick + 0.5).min(10.))),
            )
            .into_any_element()
    }

    /// Tuval sağ tık menüleri (MessageMenuPopup görünümü).
    pub(super) fn canvas_menu_layer(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.canvas.as_ref()?;
        let menu = c.menu.clone()?;
        let writable = self.doc.as_ref().is_some_and(|d| d.writable());
        let vp = window.viewport_size();
        let (at, body): (Point<Pixels>, AnyElement) = match menu {
            CanvasMenu::Node { id, at } => {
                let n = c.node(&id)?.clone();
                let has_emoji = n.extra.get("emojis").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty()) || n.extra.contains_key("emoji");
                let mut strip = div().flex().gap(px(2.)).px(px(4.)).pb(px(4.));
                for e in ["👍", "❤️", "😂", "🎉", "👀", "✅"] {
                    let nid = id.clone();
                    let hover = p.accent_soft;
                    strip = strip.child(
                        div()
                            .id(SharedString::from(format!("ne-{e}")))
                            .size(px(30.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(fs(17.))
                            .font_family(p2_ui::tokens::FONT_EMOJI)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .child(e)
                            .on_click(cx.listener(move |this, _, window, cx| this.canvas_emoji(&nid, Some(e), window, cx))),
                    );
                }
                let (n1, n2, n3, n4) = (id.clone(), id.clone(), id.clone(), id.clone());
                let mut m = p2_ui::controls::menu("canvas-node-menu", p).w(px(230.)).child(strip).child(p2_ui::controls::menu_separator(p));
                if kind(&n) != "IMAGE" && writable {
                    m = m.child(p2_ui::controls::menu_item("nm-edit", "Metni düzenle", Some(div().w(px(16.)).child("✎").into_any_element()), None, false, p).on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(c) = &mut this.canvas {
                            c.menu = None;
                        }
                        this.begin_text(n1.clone(), window, cx);
                    })));
                }
                m = m.child(p2_ui::controls::menu_item("nm-copy", "Sembolü kopyala", Some(div().w(px(16.)).child("▤").into_any_element()), None, false, p).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(c) = &mut this.canvas {
                        let node = c.node(&n2).cloned();
                        c.clipboard = node.map(|n| (vec![n], Vec::new()));
                        c.menu = None;
                    }
                    cx.notify();
                })));
                if has_emoji && writable {
                    m = m.child(p2_ui::controls::menu_item("nm-clear", "Tüm emojileri kaldır", Some(div().w(px(16.)).child("⊘").into_any_element()), None, false, p).on_click(cx.listener(move |this, _, window, cx| this.canvas_emoji(&n3, None, window, cx))));
                }
                if writable {
                    m = m.child(p2_ui::controls::menu_separator(p)).child(p2_ui::controls::menu_item("nm-del", "Sembolü sil", Some(div().w(px(16.)).child("🗑").into_any_element()), None, true, p).on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(c) = &mut this.canvas {
                            c.menu = None;
                            c.selection = vec![n4.clone()];
                        }
                        this.canvas_remove_selected(window, cx);
                    })));
                }
                (at, m.into_any_element())
            }
            CanvasMenu::Canvas { at, world } => {
                let grid = c.grid_mode();
                let (hb, hbg, hs) = (p.border_accent, p.accent_soft, p.text_strong);
                let quick = |id: &'static str, icon: AnyElement, label: &'static str| {
                    div()
                        .id(id)
                        .h(px(52.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(5.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.bg_3)
                        .text_size(fs(9.))
                        .text_color(p.text)
                        .cursor_pointer()
                        .hover(move |s| s.border_color(hb).bg(hbg).text_color(hs))
                        .child(icon)
                        .child(label)
                };
                let text2 = p.text_2;
                let shape = |round: bool| div().size(px(16.)).border_1().border_color(text2).when(round, |s| s.rounded_full()).into_any_element();
                let tri = canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
                    let mut t = PathBuilder::fill();
                    t.move_to(bb.origin + point(px(8.5), px(0.)));
                    t.line_to(bb.origin + point(px(17.), px(16.)));
                    t.line_to(bb.origin + point(px(0.), px(16.)));
                    t.close();
                    if let Ok(path) = t.build() {
                        window.paint_path(path, text2);
                    }
                })
                .w(px(17.))
                .h(px(16.))
                .into_any_element();
                let row = |a: AnyElement, b: AnyElement, c3: AnyElement| div().flex().gap(px(5.)).child(div().flex_1().child(a)).child(div().flex_1().child(b)).child(div().flex_1().child(c3));
                let has_clip = c.clipboard.as_ref().is_some_and(|(n, _)| !n.is_empty());
                let mut m = p2_ui::controls::menu("canvas-quick-menu", p).w(px(230.));
                if has_clip {
                    m = m.child(p2_ui::controls::menu_item("cq-paste", "Yapıştır", None, Some("Ctrl+V".into()), false, p).on_click(cx.listener(|this, _, window, cx| {
                        if let Some(c) = &mut this.canvas {
                            c.menu = None;
                        }
                        this.canvas_paste(window, cx);
                    })));
                    m = m.child(p2_ui::controls::menu_separator(p));
                }
                m = m.child(p2_ui::controls::menu_label("HIZLI ARAÇLAR", p)).child(
                    div()
                        .p(px(4.))
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(row(
                            quick("cq-box", shape(false), "Kutu").on_click(cx.listener(move |this, _, window, cx| this.quick_add("BOX", world, window, cx))).into_any_element(),
                            quick("cq-sphere", shape(true), "Küre").on_click(cx.listener(move |this, _, window, cx| this.quick_add("SPHERE", world, window, cx))).into_any_element(),
                            quick("cq-tri", tri, "Üçgen").on_click(cx.listener(move |this, _, window, cx| this.quick_add("TRIANGLE", world, window, cx))).into_any_element(),
                        ))
                        .child(row(
                            quick("cq-text", div().text_size(fs(17.)).font_weight(FontWeight::BOLD).line_height(px(18.)).text_color(text2).child("T").into_any_element(), "Metin").on_click(cx.listener(move |this, _, window, cx| this.quick_add("TEXT", world, window, cx))).into_any_element(),
                            quick("cq-file", line_icon("0 0 24 24", r#"<rect x="3.5" y="4" width="17" height="16" rx="2"/><circle cx="9" cy="9" r="1.5"/><path d="m5.5 17 4.2-4.6 3.1 3 2.4-2.7 3.3 4.3"/>"#, 19., 1.7, true, p.text_3).into_any_element(), "Dosya")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(c) = &mut this.canvas {
                                        c.menu = None;
                                    }
                                    this.canvas_pick_file(window, cx);
                                }))
                                .into_any_element(),
                            quick("cq-grid", tool_grid_icon(&grid, p).into_any_element(), "Grid")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(c) = &mut this.canvas {
                                        c.menu = None;
                                    }
                                    this.canvas_cycle_grid(window, cx);
                                }))
                                .into_any_element(),
                        )),
                );
                (at, m.into_any_element())
            }
        };
        let x = f32::from(at.x).min(f32::from(vp.width) - 238.).max(8.);
        let y = f32::from(at.y).min(f32::from(vp.height) - 260.).max(8.);
        Some(
            div()
                .id("canvas-menu-layer")
                .absolute()
                .inset_0()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if let Some(c) = &mut this.canvas {
                            c.menu = None;
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        if let Some(c) = &mut this.canvas {
                            c.menu = None;
                        }
                        cx.notify();
                    }),
                )
                .child(div().absolute().left(px(x)).top(px(y)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(body))
                .into_any_element(),
        )
    }

    fn quick_add(&mut self, k: &str, world: P, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = &mut self.canvas {
            c.menu = None;
        }
        self.canvas_add(k, Some(world), window, cx);
    }
}

/// `.tool-grid.(dots|lines|none)`: 17×17, 1px kenarlık; nokta/çizgi deseni ya da kırmızı çapraz.
pub(super) fn tool_grid_icon(mode: &str, p: &Palette) -> impl IntoElement {
    let (dot, line, danger, text2) = (p.text_3, p.text_4, p.danger, p.text_2);
    let mode = mode.to_string();
    div().size(px(17.)).border_1().border_color(if mode == "DOTS" { line } else { text2 }).child(
        canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
            let o = bb.origin;
            match mode.as_str() {
                "DOTS" => {
                    // 4px karolar, her karonun ortasında 1px nokta.
                    let mut y = 2.;
                    while y < 15. {
                        let mut x = 2.;
                        while x < 15. {
                            window.paint_quad(quad(Bounds::new(o + point(px(x - 1.), px(y - 1.)), size(px(2.), px(2.))), Corners::all(px(1.)), dot, Edges::default(), gpui_kit::transparent_black(), BorderStyle::default()));
                            x += 4.;
                        }
                        y += 4.;
                    }
                }
                "LINES" => {
                    let mut v = 0.;
                    while v < 15. {
                        window.paint_quad(gpui_kit::fill(Bounds::new(o + point(px(v), px(0.)), size(px(1.), px(15.))), line));
                        window.paint_quad(gpui_kit::fill(Bounds::new(o + point(px(0.), px(v)), size(px(15.), px(1.))), line));
                        v += 4.;
                    }
                }
                _ => {
                    let mut path = PathBuilder::stroke(px(1.));
                    path.move_to(o + point(px(2.), px(12.5)));
                    path.line_to(o + point(px(12.5), px(2.)));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, danger);
                    }
                }
            }
        })
        .size_full(),
    )
}

#[cfg(test)]
mod shape_tests {
    use super::*;

    #[test]
    fn ucgen_isabet_testi_sekli_izler() {
        // 210×180 üçgen: tepe (105, 0), taban (0..210, 180).
        let (w, h) = (210., 180.);
        assert!(shape_contains("TRIANGLE", w, h, 105., 120., 0.), "orta nokta içeride");
        assert!(shape_contains("TRIANGLE", w, h, 105., 2., 0.), "tepeye yakın orta");
        // Sınır kutusunun sol üst / sağ üst boş köşeleri seçmez.
        assert!(!shape_contains("TRIANGLE", w, h, 10., 10., 0.));
        assert!(!shape_contains("TRIANGLE", w, h, 200., 10., 0.));
        assert!(!shape_contains("TRIANGLE", w, h, 30., 60., 0.));
        // Kutu ise sınır kutusunun her yeri.
        assert!(shape_contains("BOX", w, h, 10., 10., 0.));
        // Tolerans kenarın biraz dışını da kabul eder.
        assert!(!shape_contains("TRIANGLE", w, h, 105., -2., 0.));
        assert!(shape_contains("TRIANGLE", w, h, 105., -2., 3.));
    }

    #[test]
    fn kure_isabet_testi_elips() {
        assert!(shape_contains("SPHERE", 180., 180., 90., 90., 0.));
        assert!(!shape_contains("SPHERE", 180., 180., 5., 5., 0.), "köşe dairenin dışında");
        assert!(shape_contains("SPHERE", 180., 180., 90., 1., 0.));
    }

    #[test]
    fn ucgen_dis_hatti_uc_kenar_ve_disari_buyur() {
        let o = shape_outline("TRIANGLE", 210., 180., 4.);
        assert_eq!(o.len(), 3);
        // Tepe yukarı, taban köşeleri dışarı ve aşağı kayar.
        assert!(o[0].1 < 0. && (o[0].0 - 105.).abs() < 1e-3);
        assert!(o[1].0 > 210. && o[1].1 > 180.);
        assert!(o[2].0 < 0. && o[2].1 > 180.);
        // Her kenar özgün kenardan tam 4px uzakta: taban y = 184.
        assert!((o[1].1 - 184.).abs() < 1e-3 && (o[2].1 - 184.).abs() < 1e-3);
        // Özgün köşeler büyütülmüş hattın içinde.
        for (x, y) in [(105., 0.5), (209.5, 179.5), (0.5, 179.5)] {
            assert!(point_in_polygon(&o, x, y));
        }
        let e = shape_outline("SPHERE", 100., 60., 2.);
        assert_eq!(e.len(), ELLIPSE_SEGMENTS);
        assert!((e[0].0 - 102.).abs() < 1e-3 && (e[0].1 - 30.).abs() < 1e-3);
    }

    #[test]
    fn secim_kutusu_ucgenin_bos_kosesini_secmez() {
        let tri = shape_outline("TRIANGLE", 210., 180., 0.);
        assert!(!polygon_intersects_rect(&tri, (0., 0., 30., 30.)));
        assert!(polygon_intersects_rect(&tri, (90., 80., 20., 20.)), "tamamen içeride");
        assert!(polygon_intersects_rect(&tri, (-50., 100., 400., 10.)), "şeridin kenarları keser");
        assert!(polygon_intersects_rect(&tri, (-10., -10., 300., 300.)), "tamamen kapsar");
    }
}
