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

#[derive(Clone, Copy, PartialEq, Debug)]
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
    /// Düğüm(ler)i taşıma: dünya başlangıcı + özgün konumlar. `insert`: tek düğüm bir bağlantının
    /// üstüne geldiyse o bağlantı (bırakınca araya eklenir).
    Move { start: P, originals: Vec<(String, f32, f32)>, remembered: bool, insert: Option<String> },
    /// Köşeden boyutlandırma.
    Resize { id: String, dir: &'static str, start: Point<Pixels>, node: CanvasNode },
    /// Seçim kutusu.
    Marquee { start: P, base: Vec<String> },
    /// Yeni bağlantı çekme (bağlantı noktasından): canlı önizleme imleci izler; `snap` yapışılacak uç.
    Connect { from: End, start: P, at: P, snap: Option<End> },
    /// Seçili bağlantının ucunu sürükleyip başka şekle / noktaya bağlama.
    Reconnect { wire: String, which: Which, remembered: bool },
    /// Dirsekli / eğri bağlantının orta (bükülme) tutamacı.
    Bend { wire: String, remembered: bool },
    /// Araç kutusundan tuvale şekil sürükleme (bağlantının üstüne bırakılırsa araya eklenir).
    Place { kind: &'static str, at: Option<P>, insert: Option<String> },
}

#[derive(Clone)]
pub(super) enum CanvasMenu {
    Node { id: String, at: Point<Pixels> },
    Canvas { at: Point<Pixels>, world: P },
    Wire { id: String, at: Point<Pixels> },
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
    /// Bağlantı etiketi düzenleme (çift tık).
    pub editing_wire: Option<(String, Entity<TextareaState>)>,
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
    /// Son çizimdeki bağlantı yolları (isabet testi, araya ekleme).
    pub paths: Vec<(String, Vec<P>)>,
    /// İmlecin üstünde durduğu bağlantı (hafif vurgu).
    pub hover_wire: Option<String>,
    /// Yeni bağlantıların biçimi: son seçilen biçim hatırlanır (Miro gibi).
    pub default_shape: WireShape,
    /// Yönlendirme önbelleği: düğüm geometrisi + bağlantılar + yok sayılanlar değişmedikçe yollar yeniden hesaplanmaz.
    pub route_cache: Option<(u64, Vec<Option<Vec<P>>>)>,
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
            editing_wire: None,
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
            paths: Vec::new(),
            hover_wire: None,
            default_shape: WireShape::Elbow,
            route_cache: None,
        }
    }

    /// Dünya noktasının üstündeki bağlantı (son çizimdeki yollarla; üstteki önce).
    fn wire_at(&self, at: P, tol: f32) -> Option<String> {
        self.paths.iter().rev().filter(|(_, r)| dist_to_polyline(r, at) <= tol).min_by(|a, b| dist_to_polyline(&a.1, at).total_cmp(&dist_to_polyline(&b.1, at))).map(|(id, _)| id.clone())
    }

    /// Düğümün şeklinden geçen (ona bağlı olmayan) bağlantı: araya ekleme hedefi. Birden çoksa
    /// şeklin merkezine en yakın olan.
    fn wire_under(&self, n: &CanvasNode) -> Option<String> {
        let center = node_center(n);
        self.paths
            .iter()
            .filter(|(id, _)| self.content.connections.iter().any(|w| &w.id == id && w.from != n.id && w.to != n.id))
            .filter(|(_, r)| path_hits_node(r, n))
            .min_by(|a, b| dist_to_polyline(&a.1, center).total_cmp(&dist_to_polyline(&b.1, center)))
            .map(|(id, _)| id.clone())
    }

    fn wire_path_of(&self, id: &str) -> Option<&Vec<P>> {
        self.paths.iter().find(|(w, _)| w == id).map(|(_, r)| r)
    }

    /// "auto" uçların eski okuyucular için yazılan tarafını tazeler.
    fn sync_sides(&mut self) {
        let nodes = &self.content.nodes;
        for w in self.content.connections.iter_mut() {
            sync_wire_sides(nodes, w);
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
fn wire_points(w: &CanvasWire) -> Vec<P> {
    w.extra.get("points").and_then(|v| v.as_array()).map(|a| a.iter().map(|p| pt(p["x"].as_f64().unwrap_or(0.) as f32, p["y"].as_f64().unwrap_or(0.) as f32)).collect()).unwrap_or_default()
}
fn json_point(v: Option<&Value>) -> Option<P> {
    let v = v?;
    Some(pt(v.get("x")?.as_f64()? as f32, v.get("y")?.as_f64()? as f32))
}
fn point_json(p: P) -> Value {
    json!({ "x": (p.x * 10.).round() / 10., "y": (p.y * 10.).round() / 10. })
}

// ------------------------------------------------------------ bağlantı (wire) modeli
//
// Kayıt biçimi (geriye dönük uyumlu; yeni alanların hepsi isteğe bağlı, eski okuyucular yok sayar):
//   from / to            düğüm kimliği; serbest uçta "" (eski okuyucu düğümü bulamaz, bağlantıyı çizmez)
//   fromSide / toSide    top|right|bottom|left — sabit bağlantı noktası (eski alan; "auto" uçta da en
//                        yakın taraf yazılır ki eski sürümler makul çizsin)
//   fromAnchor / toAnchor  "auto": uç şeklin dış hattına yapışır, yönü karşı uca göre kendisi seçer
//   fromPoint / toPoint  {x,y}: serbest uç (dünya koordinatı)
//   shape                elbow (varsayılan) | straight | curved
//   arrow                end (varsayılan) | start | both | none
//   lineStyle            solid (varsayılan) | dashed
//   bend                 {x,y}: dirsekli/eğri bağlantının orta tutamacı (bükülme noktası)
//   color, thickness, label, points  eski alanlar (points: Electron ara noktaları, aynen korunur)

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Anchor {
    Side(Side),
    Auto,
}

/// Bağlantı ucu: bir düğüme (sabit noktaya ya da dış hatta) bağlı ya da serbest.
#[derive(Clone, PartialEq, Debug)]
pub(super) enum End {
    Node { id: String, anchor: Anchor },
    Free(P),
}

impl End {
    fn node_id(&self) -> Option<&str> {
        match self {
            End::Node { id, .. } => Some(id.as_str()),
            End::Free(_) => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Which {
    From,
    To,
}

impl Which {
    /// (taraf, çapa, serbest nokta) anahtarları.
    fn keys(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Which::From => ("fromSide", "fromAnchor", "fromPoint"),
            Which::To => ("toSide", "toAnchor", "toPoint"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum WireShape {
    Elbow,
    Straight,
    Curved,
}

impl WireShape {
    fn parse(s: Option<&str>) -> Self {
        match s {
            Some("straight") => WireShape::Straight,
            Some("curved") => WireShape::Curved,
            _ => WireShape::Elbow,
        }
    }
    fn name(self) -> &'static str {
        match self {
            WireShape::Elbow => "elbow",
            WireShape::Straight => "straight",
            WireShape::Curved => "curved",
        }
    }
}

fn wire_str<'a>(w: &'a CanvasWire, key: &str) -> Option<&'a str> {
    w.extra.get(key).and_then(|v| v.as_str())
}
pub(super) fn wire_shape(w: &CanvasWire) -> WireShape {
    WireShape::parse(wire_str(w, "shape"))
}
/// (başta ok, sonda ok)
pub(super) fn wire_arrows(w: &CanvasWire) -> (bool, bool) {
    match wire_str(w, "arrow") {
        Some("none") => (false, false),
        Some("start") => (true, false),
        Some("both") => (true, true),
        _ => (false, true),
    }
}
fn wire_dashed(w: &CanvasWire) -> bool {
    wire_str(w, "lineStyle") == Some("dashed")
}
fn wire_thickness(w: &CanvasWire) -> f32 {
    w.extra.get("thickness").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(2.2)
}

pub(super) fn wire_end(w: &CanvasWire, which: Which) -> End {
    let (side_k, anchor_k, point_k) = which.keys();
    let id = match which {
        Which::From => &w.from,
        Which::To => &w.to,
    };
    if id.is_empty() {
        if let Some(p) = json_point(w.extra.get(point_k)) {
            return End::Free(p);
        }
    }
    let anchor = if wire_str(w, anchor_k) == Some("auto") { Anchor::Auto } else { Anchor::Side(Side::parse(wire_str(w, side_k))) };
    End::Node { id: id.clone(), anchor }
}

/// Ucu yazar; ilgisiz anahtarları temizler. "auto" uçlarda taraf `sync_wire_sides` ile güncellenir.
pub(super) fn store_end(w: &mut CanvasWire, which: Which, end: &End) {
    let (side_k, anchor_k, point_k) = which.keys();
    let id = match end {
        End::Node { id, anchor } => {
            w.extra.remove(point_k);
            match anchor {
                Anchor::Side(s) => {
                    w.extra.insert(side_k.into(), json!(s.name()));
                    w.extra.remove(anchor_k);
                }
                Anchor::Auto => {
                    w.extra.insert(anchor_k.into(), json!("auto"));
                    w.extra.entry(side_k.to_string()).or_insert(json!("left"));
                }
            }
            id.clone()
        }
        End::Free(p) => {
            w.extra.insert(point_k.into(), point_json(*p));
            w.extra.remove(anchor_k);
            w.extra.remove(side_k);
            String::new()
        }
    };
    match which {
        Which::From => w.from = id,
        Which::To => w.to = id,
    }
}

/// "auto" uçların `fromSide`/`toSide` alanını şu anki geometriye göre tazeler (eski okuyucular için).
pub(super) fn sync_wire_sides(nodes: &[CanvasNode], w: &mut CanvasWire) {
    let (ea, eb) = (wire_end(w, Which::From), wire_end(w, Which::To));
    let (Some(ra), Some(rb)) = (end_ref(nodes, &ea), end_ref(nodes, &eb)) else { return };
    for (which, end, other) in [(Which::From, &ea, rb), (Which::To, &eb, ra)] {
        if let End::Node { id, anchor: Anchor::Auto } = end {
            if let Some(n) = nodes.iter().find(|n| &n.id == id) {
                w.extra.insert(which.keys().0.into(), json!(facing_side(n, other).name()));
            }
        }
    }
}

/// Yeni bağlantı (biçim `shape` açıkça yazılır ki varsayılan değişse de görünüm korunur).
pub(super) fn make_wire(id: String, from: &End, to: &End, shape: WireShape, nodes: &[CanvasNode]) -> CanvasWire {
    let mut w = CanvasWire { id, from: String::new(), to: String::new(), extra: Map::new() };
    store_end(&mut w, Which::From, from);
    store_end(&mut w, Which::To, to);
    w.extra.insert("shape".into(), json!(shape.name()));
    sync_wire_sides(nodes, &mut w);
    w
}

/// Bağlantıyı düğümde ikiye böler: A→B, A→N ve N→B olur (biçim, renk, ok korunur; etiket ilk parçada
/// kalır). N bağlantının ucu ise ya da bulunamazsa hiçbir şey yapmaz.
pub(super) fn split_wire(content: &mut CanvasContent, wire_id: &str, node_id: &str, new_id: String) -> bool {
    let Some(i) = content.connections.iter().position(|w| w.id == wire_id) else { return false };
    let w = &content.connections[i];
    if w.from == node_id || w.to == node_id || !content.nodes.iter().any(|n| n.id == node_id && n.kind != "TEXT") {
        return false;
    }
    let mid = End::Node { id: node_id.to_string(), anchor: Anchor::Auto };
    let mut first = w.clone();
    let mut second = w.clone();
    second.id = new_id;
    second.extra.remove("label");
    for x in [&mut first, &mut second] {
        x.extra.remove("bend");
        x.extra.remove("points");
    }
    store_end(&mut first, Which::To, &mid);
    store_end(&mut second, Which::From, &mid);
    sync_wire_sides(&content.nodes, &mut first);
    sync_wire_sides(&content.nodes, &mut second);
    content.connections[i] = first;
    content.connections.insert(i + 1, second);
    true
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

/// Şeklin bağlantı noktası: kutu / küre / dosya için kenar ortaları; üçgende tepe (üst), yan kenarların
/// ortaları (sol / sağ) ve taban ortası (alt). Noktalar her zaman şeklin kendi hattı üzerindedir.
pub(super) fn port_point(n: &CanvasNode, s: Side) -> P {
    let (x, y, w, h) = (n.x as f32, n.y as f32, n.w as f32, n.h as f32);
    let tri = kind(n) == "TRIANGLE";
    match s {
        Side::Top => pt(x + w / 2., y),
        Side::Right if tri => pt(x + w * 0.75, y + h / 2.),
        Side::Right => pt(x + w, y + h / 2.),
        Side::Bottom => pt(x + w / 2., y + h),
        Side::Left if tri => pt(x + w * 0.25, y + h / 2.),
        Side::Left => pt(x, y + h / 2.),
    }
}

fn node_center(n: &CanvasNode) -> P {
    pt((n.x + n.w / 2.) as f32, (n.y + n.h / 2.) as f32)
}

fn side_dir(s: Side) -> (f32, f32) {
    match s {
        Side::Top => (0., -1.),
        Side::Right => (1., 0.),
        Side::Bottom => (0., 1.),
        Side::Left => (-1., 0.),
    }
}

/// Düğümün `toward` noktasına bakan tarafı (en-boy oranına göre).
pub(super) fn facing_side(n: &CanvasNode, toward: P) -> Side {
    let c = node_center(n);
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    let (w, h) = ((n.w as f32).max(1.), (n.h as f32).max(1.));
    if dx.abs() * h >= dy.abs() * w {
        if dx >= 0. { Side::Right } else { Side::Left }
    } else if dy >= 0. {
        Side::Bottom
    } else {
        Side::Top
    }
}

/// Merkezden `toward` yönündeki ışının şeklin dış hattını kestiği nokta (sınır kutusu değil, gerçek şekil).
pub(super) fn outline_toward(n: &CanvasNode, toward: P) -> P {
    let c = node_center(n);
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    if dx.hypot(dy) < 1e-3 {
        return port_point(n, Side::Top);
    }
    let (x, y) = (n.x as f32, n.y as f32);
    let poly = shape_outline(kind(n), n.w as f32, n.h as f32, 0.);
    let mut best: f32 = 0.;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let den = dx * ey - dy * ex;
        if den.abs() < 1e-9 {
            continue;
        }
        let (ax, ay) = (a.0 + x - c.x, a.1 + y - c.y);
        let t = (ax * ey - ay * ex) / den;
        let u = (ax * dy - ay * dx) / den;
        if t > 0. && (-1e-4..=1. + 1e-4).contains(&u) {
            best = best.max(t);
        }
    }
    if best <= 0. {
        return port_point(n, Side::Top);
    }
    pt(c.x + dx * best, c.y + dy * best)
}

/// Bağlantı ucunun çözülmüş hali: nokta, çıkış yönü (serbest uçta yok) ve dirsekli yolun ilk kırılmadan
/// önce şekilden uzaklaşacağı mesafe.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tip {
    pub p: P,
    pub side: Option<Side>,
    pub gap: f32,
}

fn connectable<'a>(nodes: &'a [CanvasNode], id: &str) -> Option<&'a CanvasNode> {
    nodes.iter().find(|n| n.id == id && n.kind != "TEXT")
}

/// Ucun "kaba" konumu (karşı ucun yön seçimi için): sabit noktada o nokta, "auto"da düğüm merkezi.
pub(super) fn end_ref(nodes: &[CanvasNode], e: &End) -> Option<P> {
    match e {
        End::Free(p) => Some(*p),
        End::Node { id, anchor } => {
            let n = connectable(nodes, id)?;
            Some(match anchor {
                Anchor::Side(s) => port_point(n, *s),
                Anchor::Auto => node_center(n),
            })
        }
    }
}

/// p→q doğru parçası şeklin içinden geçiyor mu (uçları hariç, kenarda 1,5 birim pay).
fn segment_enters(n: &CanvasNode, p: P, q: P) -> bool {
    let (x, y, w, h) = (n.x as f32, n.y as f32, n.w as f32, n.h as f32);
    (1..10).any(|i| {
        let t = i as f32 / 10.;
        shape_contains(kind(n), w, h, p.x + (q.x - p.x) * t - x, p.y + (q.y - p.y) * t - y, -1.5)
    })
}

fn port_gap(n: &CanvasNode, s: Side) -> f32 {
    // Üçgenin yan noktaları sınır kutusunun içinde: dirsek önce kutunun dışına çıkmalı.
    if kind(n) == "TRIANGLE" && matches!(s, Side::Left | Side::Right) { n.w as f32 * 0.25 + 18. } else { 18. }
}

fn resolve_end(nodes: &[CanvasNode], e: &End, other: P, shape: WireShape) -> Option<Tip> {
    match e {
        End::Free(p) => Some(Tip { p: *p, side: None, gap: 0. }),
        End::Node { id, anchor } => {
            let n = connectable(nodes, id)?;
            let fixed = |s: Side| Tip { p: port_point(n, s), side: Some(s), gap: port_gap(n, s) };
            let floating = || Tip { p: outline_toward(n, other), side: Some(facing_side(n, other)), gap: 18. };
            Some(match (anchor, shape) {
                (Anchor::Auto, WireShape::Elbow) => fixed(facing_side(n, other)),
                (Anchor::Auto, _) => floating(),
                (Anchor::Side(s), WireShape::Elbow) => fixed(*s),
                // Düz / eğri: sabit nokta karşı uca şeklin içinden bakıyorsa dış hatta kayar (kendi şeklini kesmez).
                (Anchor::Side(s), _) => {
                    if segment_enters(n, port_point(n, *s), other) { floating() } else { fixed(*s) }
                }
            })
        }
    }
}

fn offset_tip(t: Tip) -> P {
    match t.side {
        Some(s) => {
            let (dx, dy) = side_dir(s);
            pt(t.p.x + dx * t.gap, t.p.y + dy * t.gap)
        }
        None => t.p,
    }
}

fn compact(r: Vec<P>) -> Vec<P> {
    let mut out: Vec<P> = Vec::new();
    for p in r {
        if out.last().is_none_or(|l| (l.x - p.x).abs() > 1e-3 || (l.y - p.y).abs() > 1e-3) {
            out.push(p);
        }
    }
    out
}

/// DocumentsPage.tsx `path`: dik açılı, düğümlerin etrafından dolaşan bağlantı yolu (kırık çizgi noktaları).
/// `ignore`: engel sayılmayacak düğümler (sürüklenen düğüm başka bağlantıların yolunu oynatmasın).
pub(super) fn route(nodes: &[CanvasNode], ta: Tip, tb: Tip, points: &[P], ignore: &[&str]) -> Vec<P> {
    let (a, b) = (ta.p, tb.p);
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
    let (a_out, b_out) = (offset_tip(ta), offset_tip(tb));
    let rects: Vec<(f32, f32, f32, f32)> = nodes.iter().filter(|n| n.kind != "TEXT" && !ignore.contains(&n.id.as_str())).map(|n| (n.x as f32 - 2., (n.x + n.w) as f32 + 2., n.y as f32 - 2., (n.y + n.h) as f32 + 2.)).collect();
    let clear = |first: P, last: P| {
        rects.iter().all(|&(left, right, top, bottom)| {
            if first.x == last.x {
                !(first.x > left && first.x < right && first.y.max(last.y) > top && first.y.min(last.y) < bottom)
            } else {
                !(first.y > top && first.y < bottom && first.x.max(last.x) > left && first.x.min(last.x) < right)
            }
        })
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

/// Bükülme noktalı dirsekli yol: orta parça tutamaçtan geçer (yatay çıkışta dikey, dikey çıkışta yatay parça).
pub(super) fn elbow_bend(a: Tip, b: Tip, m: P) -> Vec<P> {
    let (a_out, b_out) = (offset_tip(a), offset_tip(b));
    let horizontal = |s: Side| matches!(s, Side::Left | Side::Right);
    let preferred = match (a.side, b.side) {
        (Some(s), _) => horizontal(s),
        (None, Some(s)) => horizontal(s),
        (None, None) => (b.p.x - a.p.x).abs() >= (b.p.y - a.p.y).abs(),
    };
    let build = |vertical_mid: bool| {
        if vertical_mid {
            compact(vec![a.p, a_out, pt(m.x, a_out.y), pt(m.x, b_out.y), b_out, b.p])
        } else {
            compact(vec![a.p, a_out, pt(a_out.x, m.y), pt(b_out.x, m.y), b_out, b.p])
        }
    };
    // Geri dönüş: çıkış noktasından sonraki parça ucun çıkış yönünün tersine gidiyorsa yol kendi şeklinin
    // içinden geçer. Tercih edilen yön geri dönüyorsa diğer yön denenir.
    let backtracks = |r: &[P]| {
        let bad = |t: Tip, out: P, next: Option<&P>| match (t.side, next) {
            (Some(s), Some(n)) => {
                let (dx, dy) = side_dir(s);
                (n.x - out.x) * dx + (n.y - out.y) * dy < -0.5
            }
            _ => false,
        };
        let i = r.iter().position(|q| (q.x - a_out.x).abs() < 1e-3 && (q.y - a_out.y).abs() < 1e-3);
        let j = r.iter().rposition(|q| (q.x - b_out.x).abs() < 1e-3 && (q.y - b_out.y).abs() < 1e-3);
        i.is_some_and(|i| bad(a, a_out, r.get(i + 1))) || j.is_some_and(|j| j > 0 && bad(b, b_out, r.get(j - 1)))
    };
    // İki yönden hangisi tutamaca (m) daha yakın geçiyorsa o: yan yana iki şekil arasında tutamacı aşağı
    // çekmek U biçimli yol verir. Eşitlikte çıkış yönüne uyan tercih edilir; geri dönen yol elenir.
    let (first, other) = (build(preferred), build(!preferred));
    let score = |r: &Vec<P>| (backtracks(r), dist_to_polyline(r, m));
    let (s1, s2) = (score(&first), score(&other));
    if (s2.0, s2.1 + 0.5) < (s1.0, s1.1) { other } else { first }
}

fn cubic(p0: P, p1: P, p2: P, p3: P, steps: usize) -> Vec<P> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1. - t;
            let (a, b, c, d) = (u * u * u, 3. * u * u * t, 3. * u * t * t, t * t * t);
            pt(a * p0.x + b * p1.x + c * p2.x + d * p3.x, a * p0.y + b * p1.y + c * p2.y + d * p3.y)
        })
        .collect()
}

/// Eğri bağlantı: uçlardan çıkış yönünde teğet kübik Bézier (bükülme noktası varsa ondan geçen iki parça).
pub(super) fn curve(a: Tip, b: Tip, bend: Option<P>) -> Vec<P> {
    let dist = (b.p.x - a.p.x).hypot(b.p.y - a.p.y).max(1.);
    let k = (dist * 0.45).clamp(24., 220.);
    let toward = |from: P, to: P| {
        let l = (to.x - from.x).hypot(to.y - from.y);
        if l < 1e-3 { (1., 0.) } else { ((to.x - from.x) / l, (to.y - from.y) / l) }
    };
    let dir = |t: Tip, other: P| t.side.map(side_dir).unwrap_or_else(|| toward(t.p, other));
    let (da, db) = (dir(a, b.p), dir(b, a.p));
    let ahead = |p: P, d: (f32, f32), k: f32| pt(p.x + d.0 * k, p.y + d.1 * k);
    match bend {
        None => cubic(a.p, ahead(a.p, da, k), ahead(b.p, db, k), b.p, 32),
        Some(m) => {
            let t = toward(a.p, b.p);
            let km = (dist * 0.22).max(20.);
            let mut v = cubic(a.p, ahead(a.p, da, k * 0.6), ahead(m, t, -km), m, 20);
            v.pop();
            v.extend(cubic(m, ahead(m, t, km), ahead(b.p, db, k * 0.6), b.p, 20));
            v
        }
    }
}

// ------------------------------------------------------------ dik açılı yönlendirici (Miro tarzı)
//
// Dirsekli bağlantılar görünürlük ızgarası üzerinde A* ile çizilir: her şekil (uçların kendi şekilleri
// dahil) `CLEARANCE` kadar büyütülmüş bir engeldir; ızgara çizgileri engel kenarları, uçların çıkış
// noktaları ve aralarındaki orta çizgiden oluşur. Maliyet = uzunluk + her dönüş için `BEND_COST`; geri
// dönüş yasak. Uçlar önce kendi şeklinin güvenlik kutusunun dışına dik çıkar (üçgenin yan noktaları
// dahil: yatay olarak kutunun dışına). Sonra 10px'den kısa ara parçalar komşu parçayı kaydırarak
// ayıklanır; neredeyse hizalı karşılıklı uçlar şeklin kenarı boyunca hizalanır (küçük zikzak olmaz).

/// Şekillerin çevresinde bırakılan boşluk (dünya birimi).
pub(super) const CLEARANCE: f32 = 16.;
/// Bir dönüşün uzunluk cinsinden bedeli: kısa ama çok dönüşlü yol yerine az dönüşlü yol seçilir.
const BEND_COST: f32 = 60.;
/// Bundan kısa ara parça (zikzak) bırakılmaz.
pub(super) const MIN_SEGMENT: f32 = 10.;
/// Başka bağlantının kullandığı şeride paralel komşu şerit aralığı.
const LANE_GAP: f32 = 12.;

/// Parça, daha önce yönlendirilmiş bağlantıların parçalarıyla üst üste biniyorsa bedeli (aynı çizgide
/// ve kesişen aralık): ikinci bağlantı komşu şeride kayar, çizgiler birbirini örtmez.
fn overlap_cost(p0: P, p1: P, occupied: &[(P, P)]) -> f32 {
    let mut cost = 0.;
    for (a, b) in occupied {
        let overlap = if (p0.y - p1.y).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3 && (p0.y - a.y).abs() < 1. {
            p0.x.max(p1.x).min(a.x.max(b.x)) - p0.x.min(p1.x).max(a.x.min(b.x))
        } else if (p0.x - p1.x).abs() < 1e-3 && (a.x - b.x).abs() < 1e-3 && (p0.x - a.x).abs() < 1. {
            p0.y.max(p1.y).min(a.y.max(b.y)) - p0.y.min(p1.y).max(a.y.min(b.y))
        } else {
            0.
        };
        if overlap > 1. {
            cost += overlap * 3. + 40.;
        }
    }
    cost
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}

impl Rect {
    fn of(n: &CanvasNode, grow: f32) -> Rect {
        Rect { l: n.x as f32 - grow, t: n.y as f32 - grow, r: (n.x + n.w) as f32 + grow, b: (n.y + n.h) as f32 + grow }
    }
    fn contains(&self, p: P) -> bool {
        p.x > self.l && p.x < self.r && p.y > self.t && p.y < self.b
    }
    fn intersects(&self, o: &Rect) -> bool {
        self.l < o.r && self.r > o.l && self.t < o.b && self.b > o.t
    }
    /// Eksene paralel parça dikdörtgenin İÇİNDEN geçiyor mu (kenar boyunca gitmek serbest).
    fn hits(&self, a: P, b: P) -> bool {
        if (a.x - b.x).abs() < 1e-3 {
            a.x > self.l && a.x < self.r && a.y.max(b.y) > self.t && a.y.min(b.y) < self.b
        } else if (a.y - b.y).abs() < 1e-3 {
            a.y > self.t && a.y < self.b && a.x.max(b.x) > self.l && a.x.min(b.x) < self.r
        } else {
            // Eğik parça (yalnız yedek yollarda): örnekleyerek.
            (0..=20).any(|i| {
                let t = i as f32 / 20.;
                self.contains(pt(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
            })
        }
    }
}

/// Engel: çarpışma dikdörtgeni ve hangi düğüme ait olduğu.
struct Obstacle<'a> {
    id: &'a str,
    rect: Rect,
}

/// Ucun güvenlik kutusunun dışındaki çıkış noktası (serbest uçta kendisi).
fn stub_point(n: Option<&CanvasNode>, t: Tip, clear: f32) -> P {
    match (n, t.side) {
        (Some(n), Some(s)) => {
            let r = Rect::of(n, clear);
            match s {
                Side::Top => pt(t.p.x, r.t.min(t.p.y - clear)),
                Side::Bottom => pt(t.p.x, r.b.max(t.p.y + clear)),
                Side::Left => pt(r.l.min(t.p.x - clear), t.p.y),
                Side::Right => pt(r.r.max(t.p.x + clear), t.p.y),
            }
        }
        _ => t.p,
    }
}

/// Bağlantı noktasını şeklin aynı kenarı boyunca kaydırır (`along`: kenar ekseni üzerindeki yeni koordinat).
/// Kenar dışına taşarsa ya da kenar kaydırılamazsa (üçgen tepesi) `None`.
pub(super) fn slide_on_side(n: &CanvasNode, s: Side, along: f32) -> Option<P> {
    let (x, y, w, h) = (n.x as f32, n.y as f32, n.w as f32, n.h as f32);
    let horizontal_side = matches!(s, Side::Top | Side::Bottom);
    match kind(n) {
        "SPHERE" => {
            let (cx, cy, rx, ry) = (x + w / 2., y + h / 2., w / 2., h / 2.);
            if horizontal_side {
                let u = (along - cx) / rx;
                if u.abs() > 0.6 {
                    return None;
                }
                let dy = ry * (1. - u * u).sqrt();
                Some(pt(along, if s == Side::Top { cy - dy } else { cy + dy }))
            } else {
                let v = (along - cy) / ry;
                if v.abs() > 0.6 {
                    return None;
                }
                let dx = rx * (1. - v * v).sqrt();
                Some(pt(if s == Side::Left { cx - dx } else { cx + dx }, along))
            }
        }
        "TRIANGLE" => match s {
            Side::Bottom => (along >= x + 8. && along <= x + w - 8.).then_some(pt(along, y + h)),
            Side::Left | Side::Right => {
                if along < y + h * 0.15 || along > y + h * 0.85 {
                    return None;
                }
                let half = w / 2. * (along - y) / h;
                Some(pt(if s == Side::Left { x + w / 2. - half } else { x + w / 2. + half }, along))
            }
            Side::Top => None,
        },
        _ => {
            if horizontal_side {
                (along >= x + 8. && along <= x + w - 8.).then_some(pt(along, if s == Side::Top { y } else { y + h }))
            } else {
                (along >= y + 8. && along <= y + h - 8.).then_some(pt(if s == Side::Left { x } else { x + w }, along))
            }
        }
    }
}

/// Kenar eksenindeki koordinat (üst/alt kenarda x, sol/sağda y).
fn along_of(s: Side, p: P) -> f32 {
    if matches!(s, Side::Top | Side::Bottom) { p.x } else { p.y }
}

/// Ardışık aynı doğrultudaki noktaları birleştirir (gereksiz ara noktalar).
fn simplify(r: Vec<P>) -> Vec<P> {
    let r = compact(r);
    let mut out: Vec<P> = Vec::with_capacity(r.len());
    for p in r {
        while out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let collinear = ((a.x - b.x).abs() < 1e-3 && (b.x - p.x).abs() < 1e-3) || ((a.y - b.y).abs() < 1e-3 && (b.y - p.y).abs() < 1e-3);
            if collinear {
                out.pop();
            } else {
                break;
            }
        }
        out.push(p);
    }
    out
}

fn seg_len(a: P, b: P) -> f32 {
    (b.x - a.x).abs() + (b.y - a.y).abs()
}

/// Yol hiçbir engelin içinden geçmiyor mu. İlk parça kaynağın, son parça hedefin kendi kutusundan
/// çıkış / giriş saplamasıdır: yalnız o şekil için muaf.
fn path_clear(r: &[P], obstacles: &[Obstacle], a_id: Option<&str>, b_id: Option<&str>) -> bool {
    let n = r.len();
    r.windows(2).enumerate().all(|(i, w)| {
        obstacles.iter().all(|o| {
            let own = (i == 0 && Some(o.id) == a_id) || (i + 2 == n && Some(o.id) == b_id);
            own || !o.rect.hits(w[0], w[1])
        })
    })
}

/// Saplamalar doğru yönde mi (ilk parça kaynağın çıkış yönünde, son parça hedefe giriş yönünde).
fn stubs_ok(r: &[P], a: Tip, b: Tip) -> bool {
    if r.len() < 2 {
        return false;
    }
    let dir_ok = |from: P, to: P, s: Option<Side>| match s {
        None => true,
        Some(s) => {
            let (dx, dy) = side_dir(s);
            let (vx, vy) = (to.x - from.x, to.y - from.y);
            let along = vx * dx + vy * dy;
            along >= 6. && (vx * dy - vy * dx).abs() < 1e-3
        }
    };
    dir_ok(r[0], r[1], a.side) && dir_ok(r[r.len() - 1], r[r.len() - 2], b.side)
}

/// Kısa ara parçaları (zikzak) komşu dik parçayı kaydırarak yok eder.
fn remove_jogs(mut r: Vec<P>, a: Tip, b: Tip, obstacles: &[Obstacle], a_id: Option<&str>, b_id: Option<&str>) -> Vec<P> {
    for _ in 0..12 {
        r = simplify(r);
        let n = r.len();
        // İlk ve son parça (saplamalar) hariç kısa parça ara.
        let Some(k) = (1..n.saturating_sub(2)).find(|&k| seg_len(r[k], r[k + 1]) < MIN_SEGMENT) else { break };
        let horizontal = (r[k].y - r[k + 1].y).abs() < 1e-3;
        let (c0, c1) = if horizontal { (r[k].x, r[k + 1].x) } else { (r[k].y, r[k + 1].y) };
        let set = |p: &mut P, v: f32| if horizontal { p.x = v } else { p.y = v };
        let mut done = false;
        // Seçenek 1: önceki dik parçayı (k-1..k) yeni konuma taşı; seçenek 2: sonrakini (k+1..k+2).
        for option in 0..2 {
            let mut c = r.clone();
            if option == 0 {
                if k < 1 {
                    continue;
                }
                set(&mut c[k], c1);
                set(&mut c[k - 1], c1);
            } else {
                if k + 2 >= n {
                    continue;
                }
                set(&mut c[k + 1], c0);
                set(&mut c[k + 2], c0);
            }
            let c = simplify(c);
            let same_ends = |q: &Vec<P>| q.first().is_some_and(|p| *p == r[0]) && q.last().is_some_and(|p| *p == r[n - 1]);
            let ortho = c.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-3 || (w[0].y - w[1].y).abs() < 1e-3);
            if same_ends(&c) && ortho && stubs_ok(&c, a, b) && path_clear(&c, obstacles, a_id, b_id) {
                r = c;
                done = true;
                break;
            }
        }
        if !done {
            break;
        }
    }
    simplify(r)
}

#[derive(Clone, Copy, PartialEq)]
struct Cost(f32);
impl Eq for Cost {}
impl PartialOrd for Cost {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cost {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.total_cmp(&self.0) // en küçük önce (BinaryHeap en büyüğü verir)
    }
}

/// Izgara koordinatlarını toplar: öncelikli olanlar (saplama uçları) her zaman kalır, diğerleri
/// kalanlara `MIN_SEGMENT`'ten yakınsa atılır (ızgara kaynaklı zikzak oluşmaz).
fn grid_coords(prio: &[f32], mid: f32, rest: impl Iterator<Item = f32>) -> Vec<f32> {
    let mut kept: Vec<f32> = Vec::new();
    for v in prio.iter().copied() {
        if !kept.iter().any(|k| (k - v).abs() < 1e-3) {
            kept.push(v);
        }
    }
    let mut others: Vec<f32> = std::iter::once(mid).chain(rest).collect();
    // Orta çizgi önce, sonra engel kenarları.
    for v in others.drain(..) {
        if !kept.iter().any(|k| (k - v).abs() < MIN_SEGMENT) {
            kept.push(v);
        }
    }
    kept.sort_by(|a, b| a.total_cmp(b));
    kept
}

/// Saplama uçları s1 → t1 arasında A* (dönüş bedelli). Dönen: s1..t1 arası köşe noktaları.
fn astar(s1: P, t1: P, a_dir: Option<Side>, b_entry: Option<Side>, rects: &[Rect], occupied: &[(P, P)]) -> Option<Vec<P>> {
    let xs = grid_coords(&[s1.x, t1.x], (s1.x + t1.x) / 2., rects.iter().flat_map(|r| [r.l, r.r]).chain(occupied.iter().filter(|(p, q)| (p.x - q.x).abs() < 1e-3).flat_map(|(p, _)| [p.x - LANE_GAP, p.x + LANE_GAP])));
    let ys = grid_coords(&[s1.y, t1.y], (s1.y + t1.y) / 2., rects.iter().flat_map(|r| [r.t, r.b]).chain(occupied.iter().filter(|(p, q)| (p.y - q.y).abs() < 1e-3).flat_map(|(p, _)| [p.y - LANE_GAP, p.y + LANE_GAP])));
    let (nx, ny) = (xs.len(), ys.len());
    let idx = |v: f32, list: &[f32]| list.iter().position(|k| (k - v).abs() < 1e-3);
    let (sx_, sy_, tx_, ty_) = (idx(s1.x, &xs)?, idx(s1.y, &ys)?, idx(t1.x, &xs)?, idx(t1.y, &ys)?);
    let at = |i: usize, j: usize| pt(xs[i], ys[j]);
    let free = |i: usize, j: usize| (i == sx_ && j == sy_) || (i == tx_ && j == ty_) || !rects.iter().any(|r| r.contains(at(i, j)));
    // Yönler: 0 +x, 1 +y, 2 -x, 3 -y; 4 = başlangıç (serbest uç, her yön serbest).
    let dir_of = |s: Side| match s {
        Side::Right => 0usize,
        Side::Bottom => 1,
        Side::Left => 2,
        Side::Top => 3,
    };
    let start_dir = a_dir.map(dir_of).unwrap_or(4);
    // Hedefe varış yönü: hedef ucun çıkış yönünün tersi.
    let entry = b_entry.map(|s| (dir_of(s) + 2) % 4);
    let state = |i: usize, j: usize, d: usize| (j * nx + i) * 5 + d;
    let mut best = vec![f32::INFINITY; nx * ny * 5];
    let mut parent: Vec<usize> = vec![usize::MAX; nx * ny * 5];
    let mut heap = std::collections::BinaryHeap::new();
    let h = |i: usize, j: usize| (xs[i] - t1.x).abs() + (ys[j] - t1.y).abs();
    let s0 = state(sx_, sy_, start_dir);
    best[s0] = 0.;
    heap.push((Cost(h(sx_, sy_)), s0));
    let mut found: Option<(f32, usize)> = None;
    while let Some((Cost(f), s)) = heap.pop() {
        if found.is_some_and(|(c, _)| f >= c) {
            break;
        }
        let g = best[s];
        if f > g + h((s / 5) % nx, (s / 5) / nx) + 1e-3 {
            continue; // eski kayıt
        }
        let (d, cell) = (s % 5, s / 5);
        let (i, j) = (cell % nx, cell / nx);
        if i == tx_ && j == ty_ {
            let extra = match (entry, d) {
                (None, _) | (_, 4) => 0.,
                (Some(e), d) if d == e => 0.,
                (Some(e), d) if (d + 2) % 4 == e => f32::INFINITY,
                _ => BEND_COST,
            };
            if g + extra < found.map_or(f32::INFINITY, |(c, _)| c) {
                found = Some((g + extra, s));
            }
            continue;
        }
        for nd in 0..4usize {
            if d < 4 && (nd + 2) % 4 == d {
                continue; // geri dönüş yok
            }
            let (ni, nj) = match nd {
                0 if i + 1 < nx => (i + 1, j),
                1 if j + 1 < ny => (i, j + 1),
                2 if i > 0 => (i - 1, j),
                3 if j > 0 => (i, j - 1),
                _ => continue,
            };
            if !free(ni, nj) {
                continue;
            }
            let (p0, p1) = (at(i, j), at(ni, nj));
            if rects.iter().any(|r| r.hits(p0, p1)) {
                continue;
            }
            let ng = g + seg_len(p0, p1) + if d < 4 && nd != d { BEND_COST } else { 0. } + overlap_cost(p0, p1, occupied);
            let ns = state(ni, nj, nd);
            if ng + 1e-3 < best[ns] {
                best[ns] = ng;
                parent[ns] = s;
                heap.push((Cost(ng + h(ni, nj)), ns));
            }
        }
    }
    let (_, mut s) = found?;
    let mut pts = Vec::new();
    loop {
        let cell = s / 5;
        pts.push(at(cell % nx, cell / nx));
        if s == s0 || parent[s] == usize::MAX {
            break;
        }
        s = parent[s];
    }
    pts.reverse();
    Some(pts)
}

/// Dirsekli bağlantı yolu: A* + saplamalar + zikzak temizliği. `None`: güvenli yol bulunamadı.
pub(super) fn ortho_route(nodes: &[CanvasNode], mut a: Tip, an: Option<&CanvasNode>, mut b: Tip, bn: Option<&CanvasNode>, ignore: &[&str], occupied: &[(P, P)]) -> Option<Vec<P>> {
    // Neredeyse hizalı karşılıklı uçlar: biri kenarı boyunca kaydırılıp hizalanır (küçük zikzak yerine düz).
    if let (Some(sa), Some(sb)) = (a.side, b.side) {
        let (da, db) = (side_dir(sa), side_dir(sb));
        let facing = da.0 == -db.0 && da.1 == -db.1 && ((b.p.x - a.p.x) * da.0 + (b.p.y - a.p.y) * da.1) > 0.;
        let off = along_of(sa, b.p) - along_of(sa, a.p);
        if facing && off.abs() > 1e-3 && off.abs() < MIN_SEGMENT * 2. {
            if let Some(q) = bn.and_then(|n| slide_on_side(n, sb, along_of(sb, a.p))) {
                b.p = q;
            } else if let Some(q) = an.and_then(|n| slide_on_side(n, sa, along_of(sa, b.p))) {
                a.p = q;
            }
        }
    }
    let (a_id, b_id) = (an.map(|n| n.id.as_str()), bn.map(|n| n.id.as_str()));
    for clear in [CLEARANCE, CLEARANCE * 0.4] {
        let obstacles: Vec<Obstacle> = nodes
            .iter()
            .filter(|n| n.kind != "TEXT" && (!ignore.contains(&n.id.as_str()) || Some(n.id.as_str()) == a_id || Some(n.id.as_str()) == b_id))
            .map(|n| Obstacle { id: n.id.as_str(), rect: Rect::of(n, clear - 1.) })
            .collect();
        let (s1, t1) = (stub_point(an, a, clear), stub_point(bn, b, clear));
        // Yakındaki engellerle dene (hız), olmazsa hepsiyle.
        let win = Rect { l: s1.x.min(t1.x) - 320., t: s1.y.min(t1.y) - 320., r: s1.x.max(t1.x) + 320., b: s1.y.max(t1.y) + 320. };
        let local: Vec<Rect> = obstacles.iter().filter(|o| o.rect.intersects(&win)).map(|o| o.rect).collect();
        let all: Vec<Rect> = obstacles.iter().map(|o| o.rect).collect();
        for rects in [&local, &all] {
            let Some(mid) = astar(s1, t1, a.side, b.side, rects, occupied) else { continue };
            let mut r = vec![a.p];
            r.extend(mid);
            r.push(b.p);
            let r = remove_jogs(r, a, b, &obstacles, a_id, b_id);
            if path_clear(&r, &obstacles, a_id, b_id) {
                return Some(r);
            }
        }
    }
    None
}

/// Paralel bağlantı şeridi: aynı iki şekil arasındaki bağlantılar üst üste binmesin diye uçlar
/// kenar boyunca (`lane` kadar) kaydırılır.
fn lane_tip(n: Option<&CanvasNode>, t: Tip, lane: f32) -> Tip {
    match (n, t.side) {
        (Some(n), Some(s)) if lane.abs() > 1e-3 => match slide_on_side(n, s, along_of(s, t.p) + lane) {
            Some(p) => Tip { p, ..t },
            None => t,
        },
        _ => t,
    }
}

/// Bağlantının çizilecek yolu (kırık çizgi; eğri de düzleştirilmiş). Uçlardan biri çözülemezse `None`.
pub(super) fn wire_path(nodes: &[CanvasNode], w: &CanvasWire, ignore: &[&str]) -> Option<Vec<P>> {
    wire_path_lane(nodes, w, ignore, 0., &[], &[])
}

/// `lane`: aynı şekil çifti arasındaki paralel bağlantının kayması (dünya birimi; kanonik yöne göre işaretli).
/// `busy`: başka bağlantıların sabit bağlantı noktaları (düğüm, taraf); auto uçlar bunlardan kaçınır.
/// `occupied`: önceden yönlendirilmiş bağlantıların parçaları; üstlerine binmek bedellidir.
pub(super) fn wire_path_lane(nodes: &[CanvasNode], w: &CanvasWire, ignore: &[&str], lane: f32, busy: &[(String, Side)], occupied: &[(P, P)]) -> Option<Vec<P>> {
    let (ea, eb) = (wire_end(w, Which::From), wire_end(w, Which::To));
    let (ra, rb) = (end_ref(nodes, &ea)?, end_ref(nodes, &eb)?);
    let shape = wire_shape(w);
    let (a, b) = (resolve_end(nodes, &ea, rb, shape)?, resolve_end(nodes, &eb, ra, shape)?);
    let (an, bn) = (ea.node_id().and_then(|id| connectable(nodes, id)), eb.node_id().and_then(|id| connectable(nodes, id)));
    let bend = json_point(w.extra.get("bend"));
    let legacy = wire_points(w);
    // Bağlantının kendi uçları her zaman engeldir (yol kendi şeklinin içinden geçmez).
    let ig: Vec<&str> = ignore.iter().copied().filter(|id| Some(*id) != ea.node_id() && Some(*id) != eb.node_id()).collect();
    Some(match shape {
        WireShape::Straight | WireShape::Curved => {
            // Paralel kayma: uçlar çizgiye dik yönde kaydırılıp dış hatta geri izdüşürülür.
            // Ters yönlü bağlantıda dik yön de terstir: şerit kanonik (kimlik sırası) yöne göre.
            let lane = if w.from > w.to { -lane } else { lane };
            let (a, b) = if lane.abs() > 1e-3 {
                let (dx, dy) = (b.p.x - a.p.x, b.p.y - a.p.y);
                let l = dx.hypot(dy).max(1e-3);
                let (px_, py_) = (-dy / l * lane, dx / l * lane);
                let shift = |n: Option<&CanvasNode>, t: Tip| match n {
                    Some(n) => Tip { p: outline_toward(n, pt(t.p.x + px_, t.p.y + py_)), ..t },
                    None => t,
                };
                (shift(an, a), shift(bn, b))
            } else {
                (a, b)
            };
            if shape == WireShape::Straight { vec![a.p, b.p] } else { curve(a, b, bend) }
        }
        WireShape::Elbow => {
            if !legacy.is_empty() {
                return Some(route(nodes, a, b, &legacy, &ig));
            }
            // Bükülme tutamacı: kullanıcının orta parçası; şekillerden geçiyorsa yönlendirici devralır.
            if let Some(m) = bend {
                let (a, b) = (lane_tip(an, a, lane), lane_tip(bn, b, lane));
                let r = elbow_bend(a, b, m);
                let obstacles: Vec<Obstacle> = nodes.iter().filter(|n| n.kind != "TEXT" && !ig.contains(&n.id.as_str())).map(|n| Obstacle { id: n.id.as_str(), rect: Rect::of(n, 2.) }).collect();
                if path_clear(&r, &obstacles, an.map(|n| n.id.as_str()), bn.map(|n| n.id.as_str())) {
                    return Some(r);
                }
            }
            // "auto" uçlarda taraf, en ucuz yolu (uzunluk + dönüş bedeli) veren taraftır; başka bağlantının
            // sabit noktası olarak dolu taraflar cezalıdır (iki bağlantı aynı noktaya binmesin).
            let candidates = |e: &End, n: Option<&CanvasNode>, other: P, base: Tip| -> Vec<(Tip, f32)> {
                match (e, n) {
                    (End::Node { anchor: Anchor::Auto, .. }, Some(n)) => {
                        let c = node_center(n);
                        let v: Vec<(Tip, f32)> = Side::ALL
                            .iter()
                            .filter(|s| {
                                let (dx, dy) = side_dir(**s);
                                (other.x - c.x) * dx + (other.y - c.y) * dy > -1e-3
                            })
                            .map(|s| (Tip { p: port_point(n, *s), side: Some(*s), gap: port_gap(n, *s) }, if busy.iter().any(|(id, bs)| *id == n.id && bs == s) { BUSY_COST } else { 0. }))
                            .collect();
                        if v.is_empty() { vec![(base, 0.)] } else { v }
                    }
                    _ => vec![(base, 0.)],
                }
            };
            let mut best: Option<(f32, Vec<P>)> = None;
            for (ta, ca) in candidates(&ea, an, rb, a) {
                for (tb, cb) in candidates(&eb, bn, ra, b) {
                    let Some(r) = ortho_route(nodes, lane_tip(an, ta, lane), an, lane_tip(bn, tb, lane), bn, &ig, occupied) else { continue };
                    let cost = route_cost(&r) + ca + cb;
                    if best.as_ref().is_none_or(|(c, _)| cost < *c - 1e-3) {
                        best = Some((cost, r));
                    }
                }
            }
            best.map(|(_, r)| r).unwrap_or_else(|| route(nodes, a, b, &[], &ig))
        }
    })
}

/// Dolu bağlantı noktası cezası (auto uç taraf seçiminde).
const BUSY_COST: f32 = 400.;

/// Yolun bedeli: uzunluk + dönüş başına `BEND_COST`.
fn route_cost(r: &[P]) -> f32 {
    r.windows(2).map(|w| seg_len(w[0], w[1])).sum::<f32>() + r.len().saturating_sub(2) as f32 * BEND_COST
}

/// Kırık çizginin uzunluğa göre orta noktası (`wireCenter`).
pub(super) fn polyline_center(r: &[P]) -> P {
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

fn dist_to_segment(p: P, a: P, b: P) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 <= 1e-9 { 0. } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0., 1.) };
    (p.x - (a.x + dx * t)).hypot(p.y - (a.y + dy * t))
}

/// Noktanın kırık çizgiye uzaklığı (bağlantı isabet testi).
pub(super) fn dist_to_polyline(r: &[P], p: P) -> f32 {
    match r.len() {
        0 => f32::MAX,
        1 => (p.x - r[0].x).hypot(p.y - r[0].y),
        _ => r.windows(2).map(|w| dist_to_segment(p, w[0], w[1])).fold(f32::MAX, f32::min),
    }
}

/// Yol düğümün şeklinden geçiyor mu (araya ekleme: sürüklenen şekil bağlantının üstünde mi).
pub(super) fn path_hits_node(r: &[P], n: &CanvasNode) -> bool {
    r.windows(2).any(|w| {
        let len = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        let steps = (len / 4.).ceil().max(1.) as usize;
        (0..=steps).any(|i| {
            let t = i as f32 / steps as f32;
            node_hit(n, pt(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t), 0.)
        })
    })
}

/// Yolun başından / sonundan verilen uzunluk kadar kırpar (ok başının altında çizgi kalmasın).
fn trim_polyline(r: &[P], start: f32, end: f32) -> Vec<P> {
    let mut v = r.to_vec();
    let cut = |v: &mut Vec<P>, mut left: f32| {
        while left > 0. && v.len() >= 2 {
            let (a, b) = (v[0], v[1]);
            let l = (b.x - a.x).hypot(b.y - a.y);
            if l > left {
                v[0] = pt(a.x + (b.x - a.x) * left / l, a.y + (b.y - a.y) * left / l);
                return;
            }
            left -= l;
            v.remove(0);
        }
    };
    cut(&mut v, start);
    v.reverse();
    cut(&mut v, end);
    v.reverse();
    v
}

/// Ok başının yönü: uç ve ucun gerisinde en az 4 birim uzaktaki ilk nokta (eğride teğete yakın).
fn behind(r: &[P]) -> Option<(P, P)> {
    let tip = *r.last()?;
    r.iter().rev().skip(1).find(|q| (q.x - tip.x).hypot(q.y - tip.y) >= 4.).map(|q| (*q, tip))
}

/// Çizim için hazırlanmış bağlantı.
#[derive(Clone)]
pub(super) struct WireDraw {
    id: String,
    path: Vec<P>,
    color: Hsla,
    thick: f32,
    dashed: bool,
    arrows: (bool, bool),
    label: Option<String>,
    /// Orta (bükülme) tutamacı var mı: dirsekli / eğri ve eski ara noktasız.
    bendable: bool,
}

/// Bağlantıyı ekran koordinatında çizer: gövde (düz / kesik), uçlarda dolu ok başları. Ok başı ölçüsü
/// kalınlıkla büyür; gövde ok başının altında kırpılır ki ucu küt görünmesin. Yol her karede ekran
/// koordinatında yeniden kurulur: yakınlaştırmada çizgiler keskin kalır.
#[allow(clippy::too_many_arguments)]
fn paint_connector(window: &mut Window, r: &[P], to_screen: &dyn Fn(P) -> Point<Pixels>, zoom: f32, color: Hsla, thick: f32, dashed: bool, arrows: (bool, bool)) {
    if r.len() < 2 {
        return;
    }
    let head = 7. + thick * 1.6;
    let body = trim_polyline(r, if arrows.0 { head * 0.75 } else { 0. }, if arrows.1 { head * 0.75 } else { 0. });
    if body.len() >= 2 {
        let width = (thick * zoom).max(1.);
        let mut b = PathBuilder::stroke(px(width));
        if dashed {
            b = b.dash_array(&[px(width * 3. + 3. * zoom), px(width * 2. + 3. * zoom)]);
        }
        b.move_to(to_screen(body[0]));
        for q in &body[1..] {
            b.line_to(to_screen(*q));
        }
        if let Ok(path) = b.build() {
            window.paint_path(path, color);
        }
    }
    let mut arrow = |from: P, tip: P| {
        let len = (tip.x - from.x).hypot(tip.y - from.y).max(0.001);
        let (ux, uy) = ((tip.x - from.x) / len, (tip.y - from.y) / len);
        let base = pt(tip.x - ux * head, tip.y - uy * head);
        let half = head * 0.5;
        let mut h = PathBuilder::fill();
        h.move_to(to_screen(tip));
        h.line_to(to_screen(pt(base.x - uy * half, base.y + ux * half)));
        h.line_to(to_screen(pt(base.x + uy * half, base.y - ux * half)));
        h.close();
        if let Ok(path) = h.build() {
            window.paint_path(path, color);
        }
    };
    if arrows.1 {
        if let Some((q, tip)) = behind(r) {
            arrow(q, tip);
        }
    }
    if arrows.0 {
        let rev: Vec<P> = r.iter().rev().copied().collect();
        if let Some((q, tip)) = behind(&rev) {
            arrow(q, tip);
        }
    }
}

/// Sürüklenen bağlantı ucunun yapışacağı yer: yakındaki bağlantı noktası (16px), değilse şeklin içi
/// ("auto", dış hatta yapışır); hiçbiri değilse `None` (serbest uç). Düz metne bağlanılmaz.
pub(super) fn snap_end(nodes: &[CanvasNode], at: P, exclude: &[&str], zoom: f32) -> Option<End> {
    let pool = || nodes.iter().rev().filter(|n| n.kind != "TEXT" && !exclude.contains(&n.id.as_str()));
    let reach = 16. / zoom;
    let mut best: Option<(f32, End)> = None;
    for n in pool() {
        for s in Side::ALL {
            let q = port_point(n, s);
            let d = (q.x - at.x).hypot(q.y - at.y);
            if d <= reach && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, End::Node { id: n.id.clone(), anchor: Anchor::Side(s) }));
            }
        }
    }
    if let Some((_, e)) = best {
        return Some(e);
    }
    pool().find(|n| node_hit(n, at, 2. / zoom)).map(|n| End::Node { id: n.id.clone(), anchor: Anchor::Auto })
}

/// Türe göre varsayılan ölçüyle, merkezi `center` olan yeni düğüm (Electron `add`).
pub(super) fn new_node(kind_: &str, center: P) -> CanvasNode {
    let (w, h) = match kind_ {
        "SPHERE" => (180., 180.),
        "TRIANGLE" => (210., 180.),
        "IMAGE" => (420., 300.),
        "TEXT" => (140., 37.),
        _ => (280., 180.),
    };
    let mut extra = Map::new();
    extra.insert("borderWidth".into(), json!(if kind_ == "TEXT" { 0 } else { 2 }));
    extra.insert("borderColor".into(), json!("#000000"));
    extra.insert("fontSize".into(), json!(if kind_ == "TEXT" { 18 } else { 14 }));
    CanvasNode {
        id: uid(),
        kind: kind_.into(),
        x: (center.x - w / 2.) as f64,
        y: (center.y - h / 2.) as f64,
        w: w as f64,
        h: h as f64,
        text: Some(String::new()),
        color: Some(if kind_ == "TEXT" { "transparent".into() } else { "#1d2f57".into() }),
        extra,
    }
}

/// Paletteki sabit renkler (DocumentsPage.tsx `colors`).
const SWATCHES: [&str; 8] = ["#26345f", "#315b8a", "#2f6a5b", "#6b3f7d", "#8a5b31", "#7d3643", "#d8dbe8", "#202431"];
/// Bağlantı renkleri: varsayılan mavi + paletin tonları.
const WIRE_SWATCHES: [&str; 8] = ["#4f9dff", "#8da0e7", "#2f6a5b", "#6b3f7d", "#8a5b31", "#7d3643", "#d8dbe8", "#202431"];

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
    /// Düğüm ekler (araç kutusu, hızlı menü, otomatik pilot).
    pub(super) fn canvas_add(&mut self, kind_: &str, at: Option<P>, window: &mut Window, cx: &mut Context<Self>) {
        self.canvas_place(kind_, at, None, window, cx);
    }

    /// Düğüm ekler (Electron `add`): ölçü türe göre, konum görünür alanın ortası ya da verilen nokta.
    /// `insert`: yeni şekil bu bağlantının araya eklenir (A→B, A→yeni→B olur).
    fn canvas_place(&mut self, kind_: &str, at: Option<P>, insert: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Option<String> {
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return None;
        }
        let c = self.canvas.as_mut()?;
        c.drag = CanvasDrag::None;
        let b = c.bounds.get();
        let center = at.unwrap_or_else(|| pt(c.view.x + f32::from(b.size.width) / c.zoom / 2., c.view.y + f32::from(b.size.height) / c.zoom / 2.));
        c.remember();
        let node = new_node(kind_, center);
        let id = node.id.clone();
        c.content.nodes.push(node);
        if let Some(wire) = insert {
            if split_wire(&mut c.content, &wire, &id, uid()) {
                crate::trace("doküman: şekil bağlantının araya eklendi");
            }
        }
        c.selection = vec![id.clone()];
        c.selected_wire = None;
        c.format_suppressed = false;
        self.schedule_save(window, cx);
        if kind_ == "TEXT" {
            self.begin_text(id.clone(), window, cx);
        }
        Some(id)
    }

    /// Araç kutusundan sürükleme başlar: tuvale bırakınca şekil orada oluşur (bağlantı üstüyse araya eklenir).
    fn begin_place(&mut self, k: &'static str) {
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return;
        }
        if let Some(c) = &mut self.canvas {
            c.drag = CanvasDrag::Place { kind: k, at: None, insert: None };
            c.menu = None;
        }
    }

    /// Yeni bağlantıyı kaydeder ve seçer.
    fn canvas_connect(&mut self, from: End, to: End, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        c.remember();
        let id = uid();
        let wire = make_wire(id.clone(), &from, &to, c.default_shape, &c.content.nodes);
        crate::trace(&format!("doküman: bağlantı eklendi {} → {}", from.node_id().unwrap_or("serbest"), to.node_id().unwrap_or("serbest")));
        c.content.connections.push(wire);
        c.selection.clear();
        c.selected_wire = Some(id);
        self.schedule_save(window, cx);
    }

    /// Hızlı oluşturma (Miro'daki kenar okları): seçili şeklin o yönüne aynı türde, aynı biçimde bir şekil
    /// ekler ve ikisini bağlar. Hedef doluysa boş yer bulunana dek kaydırılır.
    fn canvas_quick_create(&mut self, id: &str, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return;
        }
        let Some(c) = &mut self.canvas else { return };
        let Some(src) = c.node(id).cloned() else { return };
        let (dx, dy) = side_dir(side);
        let step = if dx != 0. { src.w as f32 + 110. } else { src.h as f32 + 110. };
        let mut copy = src.clone();
        copy.id = uid();
        copy.text = Some(String::new());
        copy.extra.remove("emoji");
        copy.extra.remove("emojis");
        let free = |x: f64, y: f64, nodes: &[CanvasNode]| !nodes.iter().any(|n| x < n.x + n.w + 20. && x + src.w > n.x - 20. && y < n.y + n.h + 20. && y + src.h > n.y - 20.);
        let mut k = 1.;
        loop {
            copy.x = src.x + (dx * step * k) as f64;
            copy.y = src.y + (dy * step * k) as f64;
            if free(copy.x, copy.y, &c.content.nodes) || k >= 8. {
                break;
            }
            // Doluysa önce yana kaydır, sonra daha uzağa.
            copy.x += (dy.abs() * (src.w as f32 + 40.) * (k - 0.5).signum()) as f64;
            copy.y += (dx.abs() * (src.h as f32 + 40.) * (k - 0.5).signum()) as f64;
            if free(copy.x, copy.y, &c.content.nodes) {
                break;
            }
            k += 1.;
        }
        c.remember();
        let new_id = copy.id.clone();
        c.content.nodes.push(copy);
        let opposite = match side {
            Side::Top => Side::Bottom,
            Side::Right => Side::Left,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
        };
        let wire = make_wire(uid(), &End::Node { id: id.to_string(), anchor: Anchor::Side(side) }, &End::Node { id: new_id.clone(), anchor: Anchor::Side(opposite) }, c.default_shape, &c.content.nodes);
        c.content.connections.push(wire);
        c.selection = vec![new_id];
        c.selected_wire = None;
        c.format_suppressed = false;
        crate::trace("doküman: hızlı şekil eklendi");
        self.schedule_save(window, cx);
    }

    /// Bağlantının ortasına yeni şekil ekleyip ikiye böler (bağlantı araç çubuğundaki "+").
    fn canvas_insert_on_wire(&mut self, wire: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &self.canvas else { return };
        let Some(r) = c.wire_path_of(wire) else { return };
        let mid = polyline_center(r);
        // Yeni şekil kaynağın türünde (düz metin ve dosya değilse), yoksa kutu.
        let src_kind = c.content.connections.iter().find(|w| w.id == wire).and_then(|w| c.node(&w.from)).map(|n| kind(n).to_string()).filter(|k| k != "IMAGE" && k != "TEXT").unwrap_or_else(|| "BOX".into());
        let k: &'static str = match src_kind.as_str() {
            "SPHERE" => "SPHERE",
            "TRIANGLE" => "TRIANGLE",
            _ => "BOX",
        };
        if let Some(c) = &mut self.canvas {
            c.menu = None;
        }
        self.canvas_place(k, Some(mid), Some(wire.to_string()), window, cx);
    }

    /// Bağlantı etiketini düzenlemeye başlar (çift tık / menü).
    fn begin_wire_label(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_text(window, cx);
        let Some(c) = &mut self.canvas else { return };
        let text = c.content.connections.iter().find(|w| w.id == id).and_then(|w| wire_str(w, "label").map(str::to_string)).unwrap_or_default();
        let editor = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).placeholder("Etiket…"));
        editor.update(cx, |t, cx| {
            t.set_value(text, window, cx);
            t.focus(window, cx);
        });
        self._subs.push(cx.subscribe_in(&editor, window, |_, _, e: &InputEvent, _, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        }));
        c.menu = None;
        c.selection.clear();
        c.selected_wire = Some(id.clone());
        c.editing_wire = Some((id, editor));
        cx.notify();
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
            c.editing_wire = None;
            c.content.connections.retain(|x| x.id != w);
            crate::trace("doküman: bağlantı silindi");
        } else if !c.selection.is_empty() {
            // Miro gibi: aradaki düğüm silinince ona bağlı bağlantılar da silinir (yeniden bağlanmaz).
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
        if !self.doc.as_ref().is_some_and(|d| d.writable()) {
            return;
        }
        let Some(c) = &mut self.canvas else { return };
        let next = if redo { c.redo.pop() } else { c.undo.pop() };
        let Some(next) = next else { return };
        let current = std::mem::replace(&mut c.content, next);
        if redo { c.undo.push(current) } else { c.redo.push(current) }
        c.selection.retain(|id| c.content.nodes.iter().any(|n| &n.id == id));
        c.selected_wire = c.selected_wire.take().filter(|id| c.content.connections.iter().any(|w| &w.id == id));
        c.editing = None;
        c.editing_wire = None;
        c.drag = CanvasDrag::None;
        crate::trace(if redo { "doküman: yinele" } else { "doküman: geri al" });
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
        c.menu = None;
        if let Some(shape) = patch.get("shape").and_then(|v| v.as_str()) {
            // Yeni bağlantılar son seçilen biçimle çizilir.
            c.default_shape = WireShape::parse(Some(shape));
        }
        if let Some(w) = c.content.connections.iter_mut().find(|w| w.id == id) {
            if let Some(obj) = patch.as_object() {
                for (k, v) in obj {
                    // null: alanı kaldır (ör. bükülmeyi sıfırla).
                    if v.is_null() {
                        w.extra.remove(k);
                    } else {
                        w.extra.insert(k.clone(), v.clone());
                    }
                }
            }
            // Biçim değişince eski bükülme noktası anlamını yitirir.
            if patch.get("shape").is_some() {
                w.extra.remove("bend");
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
            let busy = c.editing.is_some() || c.editing_wire.is_some();
            // Yarım kalan bağlantı çekme / şekil bırakma iptal; düzenleme yoksa seçim de kalkar.
            if matches!(c.drag, CanvasDrag::Connect { .. } | CanvasDrag::Place { .. }) {
                c.drag = CanvasDrag::None;
            } else if !busy {
                c.selection.clear();
                c.selected_wire = None;
            }
            self.commit_text(window, cx);
            cx.notify();
            return;
        }
        if c.editing.is_some() || c.editing_wire.is_some() {
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
            (true, "a") => {
                c.selection = c.content.nodes.iter().map(|n| n.id.clone()).collect();
                c.selected_wire = None;
                c.format_suppressed = true;
            }
            (_, "delete" | "backspace") if writable => self.canvas_remove_selected(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn canvas_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        self.commit_text(window, cx);
        let writable = self.doc.as_ref().is_some_and(|d| d.writable());
        let Some(c) = &mut self.canvas else { return };
        c.menu = None;
        let at = c.world(e.position);
        // Bağlantı isabeti: yol boyunca 7px pay (düğümlerin dışında kalan kısım; düğüm tıklamayı önce alır).
        if let Some(id) = c.wire_at(at, 7. / c.zoom) {
            c.selection.clear();
            c.selected_wire = Some(id.clone());
            c.format_suppressed = false;
            c.drag = CanvasDrag::None;
            if e.click_count >= 2 && writable {
                self.begin_wire_label(id, window, cx);
            }
            cx.notify();
            return;
        }
        c.selected_wire = None;
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
            CanvasDrag::Move { start, originals, remembered, .. } => {
                let (dx, dy) = (at.x - start.x, at.y - start.y);
                if dx.abs() < 0.5 && dy.abs() < 0.5 {
                    return;
                }
                if !remembered {
                    c.remember();
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
                // Tek şekil bir bağlantının üstüne sürüklendiyse o bağlantı vurgulanır, bırakınca araya eklenir.
                let insert = if originals.len() == 1 { c.node(&originals[0].0).cloned().and_then(|n| c.wire_under(&n)) } else { None };
                c.drag = CanvasDrag::Move { start, originals, remembered: true, insert };
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
            CanvasDrag::Connect { from, start, .. } => {
                let exclude: Vec<&str> = from.node_id().into_iter().collect();
                let snap = snap_end(&c.content.nodes, at, &exclude, c.zoom);
                c.drag = CanvasDrag::Connect { from, start, at, snap };
                cx.notify();
            }
            CanvasDrag::Reconnect { wire, which, remembered } => {
                if !remembered {
                    c.remember();
                }
                // Karşı ucun düğümüne bağlanmaz (kendine dönen bağlantı yok).
                let other = c.content.connections.iter().find(|w| w.id == wire).map(|w| wire_end(w, if which == Which::From { Which::To } else { Which::From }));
                let exclude: Vec<&str> = other.as_ref().and_then(|o| o.node_id()).into_iter().collect();
                let end = snap_end(&c.content.nodes, at, &exclude, c.zoom).unwrap_or(End::Free(at));
                let content = &mut c.content;
                if let Some(w) = content.connections.iter_mut().find(|w| w.id == wire) {
                    store_end(w, which, &end);
                    w.extra.remove("bend");
                    sync_wire_sides(&content.nodes, w);
                }
                c.drag = CanvasDrag::Reconnect { wire, which, remembered: true };
                cx.notify();
            }
            CanvasDrag::Bend { wire, remembered } => {
                if !remembered {
                    c.remember();
                }
                if let Some(w) = c.content.connections.iter_mut().find(|w| w.id == wire) {
                    w.extra.insert("bend".into(), point_json(at));
                }
                c.drag = CanvasDrag::Bend { wire, remembered: true };
                cx.notify();
            }
            CanvasDrag::Place { kind, .. } => {
                let insert = c.wire_under(&new_node(kind, at));
                c.drag = CanvasDrag::Place { kind, at: Some(at), insert };
                cx.notify();
            }
            CanvasDrag::None => {
                // Üzerine gelinen bağlantı hafifçe vurgulanır (tıklanabilir olduğu görülsün).
                let hover = c.wire_at(at, 7. / c.zoom);
                if hover != c.hover_wire {
                    c.hover_wire = hover;
                    cx.notify();
                }
            }
        }
    }

    fn canvas_up(&mut self, e: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = &mut self.canvas else { return };
        let drag = std::mem::replace(&mut c.drag, CanvasDrag::None);
        c.marquee = None;
        match drag {
            CanvasDrag::Move { remembered: true, originals, insert, .. } => {
                crate::trace("doküman: düğüm taşındı");
                if let (Some(wire), [(id, ..)]) = (insert, &originals[..]) {
                    if split_wire(&mut c.content, &wire, id, uid()) {
                        crate::trace("doküman: şekil bağlantının araya eklendi");
                    }
                }
                c.sync_sides();
                self.schedule_save(window, cx);
            }
            CanvasDrag::Resize { .. } => {
                crate::trace("doküman: düğüm boyutlandırıldı");
                c.sync_sides();
                self.schedule_save(window, cx);
            }
            CanvasDrag::Reconnect { remembered: true, .. } => {
                crate::trace("doküman: bağlantı ucu yeniden bağlandı");
                self.schedule_save(window, cx);
            }
            CanvasDrag::Bend { remembered: true, .. } => {
                crate::trace("doküman: bağlantı büküldü");
                self.schedule_save(window, cx);
            }
            CanvasDrag::Pan { moved: false, .. } if e.button == MouseButton::Right => {
                // Kısa sağ tık: düğüm menüsü, bağlantı menüsü ya da hızlı araçlar.
                let at = c.world(e.position);
                let hit = c.content.nodes.iter().rev().find(|n| node_hit(n, at, 0.)).map(|n| n.id.clone());
                let wire = if hit.is_none() { c.wire_at(at, 7. / c.zoom) } else { None };
                c.menu = Some(match (hit, wire) {
                    (Some(id), _) => {
                        c.selection = vec![id.clone()];
                        c.selected_wire = None;
                        c.format_suppressed = true;
                        CanvasMenu::Node { id, at: e.position }
                    }
                    (None, Some(id)) => {
                        c.selection.clear();
                        c.selected_wire = Some(id.clone());
                        CanvasMenu::Wire { id, at: e.position }
                    }
                    (None, None) => CanvasMenu::Canvas { at: e.position, world: at },
                });
                cx.notify();
            }
            CanvasDrag::Connect { from, start, at, snap } => {
                // Kısa tık (sürüklemeden) iptal; boşa bırakınca uç serbest kalır (Miro), şekle bırakınca bağlanır.
                if (at.x - start.x).hypot(at.y - start.y) * c.zoom >= 6. {
                    let to = snap.unwrap_or(End::Free(at));
                    self.canvas_connect(from, to, window, cx);
                }
                cx.notify();
            }
            CanvasDrag::Place { kind, at: Some(at), insert } => {
                self.canvas_place(kind, Some(at), insert, window, cx);
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
            c.drag = CanvasDrag::Move { start, originals, remembered: false, insert: None };
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
        let mut changed = false;
        if let Some((id, editor)) = c.editing.take() {
            let text = editor.read(cx).value().to_string();
            if c.node(&id).is_some_and(|n| n.text.as_deref() != Some(text.as_str())) {
                // Metin değişikliği de geri alınabilir.
                c.remember();
                if let Some(n) = c.content.nodes.iter_mut().find(|n| n.id == id) {
                    n.text = Some(text);
                }
                changed = true;
            }
        }
        if let Some((id, editor)) = c.editing_wire.take() {
            let text = editor.read(cx).value().trim().to_string();
            let old = c.content.connections.iter().find(|w| w.id == id).map(|w| wire_str(w, "label").unwrap_or_default().to_string());
            if old.is_some_and(|o| o != text) {
                c.remember();
                if let Some(w) = c.content.connections.iter_mut().find(|w| w.id == id) {
                    if text.is_empty() {
                        w.extra.remove("label");
                    } else {
                        w.extra.insert("label".into(), json!(text));
                    }
                }
                crate::trace("doküman: bağlantı etiketi kaydedildi");
                changed = true;
            }
        }
        if changed {
            self.schedule_save(window, cx);
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

        // ---- bağlantı yolları: çizim, isabet testi ve araya ekleme aynı yolu kullanır (önbelleğe yazılır).
        // Sürüklenen düğümler başka bağlantılar için engel sayılmaz: üstlerinden geçerken yol zıplamaz,
        // araya ekleme vurgusu da sabit yola göre çalışır.
        let moving: Vec<String> = match &c.drag {
            CanvasDrag::Move { originals, .. } => originals.iter().map(|o| o.0.clone()).collect(),
            _ => Vec::new(),
        };
        let ignore: Vec<&str> = moving.iter().map(String::as_str).collect();
        // Aynı şekil çifti arasındaki bağlantılar üst üste binmesin: her birine şerit kayması (14 birim).
        let lanes: Vec<f32> = {
            let conns = &c.content.connections;
            let key = |w: &CanvasWire| -> (String, String) { if w.from <= w.to { (w.from.clone(), w.to.clone()) } else { (w.to.clone(), w.from.clone()) } };
            conns
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    if w.from.is_empty() || w.to.is_empty() {
                        return 0.;
                    }
                    let group: Vec<usize> = conns.iter().enumerate().filter(|(_, x)| !x.from.is_empty() && !x.to.is_empty() && key(x) == key(w)).map(|(j, _)| j).collect();
                    let pos = group.iter().position(|j| *j == i).unwrap_or(0) as f32;
                    (pos - (group.len() as f32 - 1.) / 2.) * 14.
                })
                .collect()
        };
        // Başka bağlantıların sabit noktaları: auto uçlar bu noktalara binmez.
        let busy: Vec<(String, Side)> = c.content.connections.iter().flat_map(|w| [Which::From, Which::To].into_iter().filter_map(move |k| match wire_end(w, k) {
            End::Node { id, anchor: Anchor::Side(s) } if !id.is_empty() => Some((id, s)),
            _ => None,
        })).collect();
        // Yönlendirme pahalı (A*): girdiler değişmediyse önceki yollar kullanılır.
        let route_key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for n in &nodes {
                (n.id.as_str(), n.kind.as_str(), n.x.to_bits(), n.y.to_bits(), n.w.to_bits(), n.h.to_bits()).hash(&mut h);
            }
            serde_json::to_string(&c.content.connections).unwrap_or_default().hash(&mut h);
            ignore.hash(&mut h);
            h.finish()
        };
        let routed: Vec<Option<Vec<P>>> = match &c.route_cache {
            Some((k, r)) if *k == route_key && r.len() == c.content.connections.len() => r.clone(),
            _ => {
                // Sırayla yönlendirilir: her dirsekli yol sonrakiler için "dolu şerit" olur (üst üste binmez).
                let mut occupied: Vec<(P, P)> = Vec::new();
                let r: Vec<Option<Vec<P>>> = c
                    .content
                    .connections
                    .iter()
                    .zip(&lanes)
                    .map(|(w, lane)| {
                        let r = wire_path_lane(&nodes, w, &ignore, *lane, &busy, &occupied);
                        if let Some(r) = r.as_ref().filter(|_| wire_shape(w) == WireShape::Elbow) {
                            occupied.extend(r.windows(2).map(|s| (s[0], s[1])));
                        }
                        r
                    })
                    .collect();
                c.route_cache = Some((route_key, r.clone()));
                r
            }
        };
        let wires: Vec<WireDraw> = c
            .content
            .connections
            .iter()
            .zip(routed)
            .filter_map(|(w, path)| {
                let path = path?;
                Some(WireDraw {
                    id: w.id.clone(),
                    path,
                    color: parse_color(wire_str(w, "color")).unwrap_or(rgb(0x4f9dff).into()),
                    thick: wire_thickness(w),
                    dashed: wire_dashed(w),
                    arrows: wire_arrows(w),
                    label: wire_str(w, "label").filter(|s| !s.trim().is_empty()).map(str::to_string),
                    bendable: wire_shape(w) != WireShape::Straight && wire_points(w).is_empty(),
                })
            })
            .collect();
        c.paths = wires.iter().map(|w| (w.id.clone(), w.path.clone())).collect();
        if c.hover_wire.as_ref().is_some_and(|h| !c.paths.iter().any(|(id, _)| id == h)) {
            c.hover_wire = None;
        }
        // Çekilmekte olan bağlantının canlı önizlemesi (gerçek bağlantıyla aynı geometri) ve hedef şekil.
        let draft: Option<Vec<P>> = match &c.drag {
            CanvasDrag::Connect { from, at, snap, .. } => {
                let to = snap.clone().unwrap_or(End::Free(*at));
                wire_path(&nodes, &make_wire(String::new(), from, &to, c.default_shape, &nodes), &[])
            }
            _ => None,
        };
        let target_node: Option<String> = match &c.drag {
            CanvasDrag::Connect { snap, .. } => snap.as_ref().and_then(|e| e.node_id()).map(str::to_string),
            CanvasDrag::Reconnect { wire, which, .. } => c.content.connections.iter().find(|w| &w.id == wire).and_then(|w| wire_end(w, *which).node_id().map(str::to_string)),
            _ => None,
        };
        let connecting = matches!(c.drag, CanvasDrag::Connect { .. } | CanvasDrag::Reconnect { .. });
        let dragging_nodes = matches!(c.drag, CanvasDrag::Move { remembered: true, .. } | CanvasDrag::Resize { .. } | CanvasDrag::Place { .. });
        let insert_wire: Option<String> = match &c.drag {
            CanvasDrag::Move { insert, .. } | CanvasDrag::Place { insert, .. } => insert.clone(),
            _ => None,
        };
        // Araç kutusundan sürüklenen şeklin hayaleti (dünya koordinatlı dış hat).
        let ghost: Option<Vec<P>> = match &c.drag {
            CanvasDrag::Place { kind: k, at: Some(at), .. } => {
                let n = new_node(k, *at);
                Some(shape_outline(kind(&n), n.w as f32, n.h as f32, 0.).into_iter().map(|(x, y)| pt(n.x as f32 + x, n.y as f32 + y)).collect())
            }
            _ => None,
        };

        // ---- ızgara + bağlantılar (tek boyama katmanı)
        let grid = c.grid_mode();
        let grid_color = parse_color(c.content.extra.get("gridColor").and_then(|v| v.as_str())).unwrap_or(rgb(0xdfe5f0).into());
        let spacing = c.content.extra.get("gridSpacing").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(40.).clamp(10., 100.);
        let opacity = c.content.extra.get("gridOpacity").and_then(|v| v.as_f64()).map(|v| v as f32).unwrap_or(0.1).clamp(0.02, 0.4);
        let selected_wire = c.selected_wire.clone();
        let hover_wire = c.hover_wire.clone().filter(|_| matches!(c.drag, CanvasDrag::None));
        let (sel_main, sel_under) = selection_colors(p);
        let accent = p.accent_hover;
        let wire_data = wires.clone();
        let selected_wire_paint = selected_wire.clone();
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
                for wd in &wire_data {
                    // Vurgu katmanı (altta): araya ekleme hedefi vurgu renginde, seçili / üzerine gelinen nötr gri.
                    let halo = if insert_wire.as_deref() == Some(wd.id.as_str()) {
                        Some((accent, 0.6, 9.))
                    } else if selected_wire_paint.as_deref() == Some(wd.id.as_str()) {
                        Some((sel_main, 0.38, 7.))
                    } else if hover_wire.as_deref() == Some(wd.id.as_str()) {
                        Some((sel_main, 0.2, 6.))
                    } else {
                        None
                    };
                    if let Some((mut col, a, extra)) = halo {
                        col.a = a;
                        paint_connector(window, &wd.path, &to_screen, zoom, col, wd.thick + extra, false, (false, false));
                    }
                    paint_connector(window, &wd.path, &to_screen, zoom, wd.color, wd.thick, wd.dashed, wd.arrows);
                }
                if let Some(r) = &draft {
                    let mut col = accent;
                    col.a = 0.95;
                    paint_connector(window, r, &to_screen, zoom, col, 2.2, false, (false, true));
                }
                if let Some(poly) = &ghost {
                    let screen: Vec<(f32, f32)> = poly.iter().map(|q| (sx(q.x), sy(q.y))).collect();
                    let mut fill = accent;
                    fill.a = 0.12;
                    if let Some(first) = screen.first() {
                        let mut f = PathBuilder::fill();
                        f.move_to(o + point(px(first.0), px(first.1)));
                        for (x, y) in &screen[1..] {
                            f.line_to(o + point(px(*x), px(*y)));
                        }
                        f.close();
                        if let Ok(path) = f.build() {
                            window.paint_path(path, fill);
                        }
                    }
                    stroke_polygon(window, o, &screen, 1.5, accent);
                }
                let _ = sel_under;
            },
        )
        .absolute()
        .inset_0();

        // ---- bağlantı etiketleri: yolun orta noktasında, zemin renginde hap (çizgiyi keser, okunur kalır).
        let editing_wire = c.editing_wire.clone();
        let mut labels = Vec::new();
        for wd in &wires {
            let m = polyline_center(&wd.path);
            if let Some((eid, editor)) = editing_wire.as_ref().filter(|(eid, _)| *eid == wd.id) {
                let _ = eid;
                labels.push(
                    div()
                        .absolute()
                        .left(px(sx(m.x) - 90.))
                        .top(px(sy(m.y) - 16.))
                        .w(px(180.))
                        .px(px(6.))
                        .py(px(3.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(p.border_accent)
                        .bg(p.bg_1)
                        .shadow(p.shadow_lg())
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(Textarea::new(editor).appearance(false).text_size(px(12.)).text_color(p.text_strong))
                        .into_any_element(),
                );
                continue;
            }
            let Some(label) = wd.label.clone() else { continue };
            let (wid, wid2) = (wd.id.clone(), wd.id.clone());
            let lh = 16. * zoom;
            labels.push(
                div()
                    .absolute()
                    .left(px(sx(m.x) - 150.))
                    .w(px(300.))
                    // Seçiliyken etiket orta tutamacın altına kayar (tutamaç etiketi örtmesin).
                    .top(px(sy(m.y) - lh / 2. - 2. * zoom + if selected_wire.as_deref() == Some(wd.id.as_str()) { lh / 2. + 12. } else { 0. }))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id(SharedString::from(format!("wire-label-{}", wd.id)))
                            .max_w(px(300.))
                            .px(px(5. * zoom))
                            .py(px(2. * zoom))
                            .rounded(px(4. * zoom))
                            .bg(p.bg_0)
                            .text_size(fs(12.) * zoom)
                            .line_height(px(lh))
                            .font_family("Arial")
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_strong)
                            .cursor_pointer()
                            .child(label)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    window.focus(&this.focus, cx);
                                    this.commit_text(window, cx);
                                    let writable = this.doc.as_ref().is_some_and(|d| d.writable());
                                    if let Some(c) = &mut this.canvas {
                                        c.selection.clear();
                                        c.selected_wire = Some(wid.clone());
                                        c.menu = None;
                                    }
                                    if e.click_count >= 2 && writable {
                                        this.begin_wire_label(wid2.clone(), window, cx);
                                    }
                                    cx.notify();
                                }),
                            ),
                    )
                    .into_any_element(),
            );
        }

        // ---- seçili bağlantının tutamaçları: uçlar (sürükle → başka şekle bağla), orta (bükülme).
        let mut wire_handles = Vec::new();
        if let Some(wd) = wires.iter().find(|w| selected_wire.as_deref() == Some(w.id.as_str())).filter(|_| writable && editing_wire.is_none()) {
            let (first, last) = (wd.path[0], *wd.path.last().unwrap_or(&wd.path[0]));
            for (which, q) in [(Which::From, first), (Which::To, last)] {
                let wid = wd.id.clone();
                wire_handles.push(
                    div()
                        .id(SharedString::from(format!("wire-end-{}-{:?}", wd.id, which)))
                        .absolute()
                        .left(px(sx(q.x) - 7.))
                        .top(px(sy(q.y) - 7.))
                        .size(px(14.))
                        .rounded_full()
                        .border_2()
                        .border_color(sel_under)
                        .bg(sel_main)
                        .cursor_crosshair()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                window.focus(&this.focus, cx);
                                if let Some(c) = &mut this.canvas {
                                    c.menu = None;
                                    c.drag = CanvasDrag::Reconnect { wire: wid.clone(), which, remembered: false };
                                }
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                );
            }
            if wd.bendable {
                let m = polyline_center(&wd.path);
                let (wid, wid2) = (wd.id.clone(), wd.id.clone());
                wire_handles.push(
                    div()
                        .id(SharedString::from(format!("wire-bend-{}", wd.id)))
                        .absolute()
                        .left(px(sx(m.x) - 6.))
                        .top(px(sy(m.y) - 6.))
                        .size(px(12.))
                        .rounded(px(3.))
                        .border_2()
                        .border_color(sel_main)
                        .bg(sel_under)
                        .cursor_move()
                        .tooltip(|window, cx| p2_ui::controls::tooltip("Bükmek için sürükle · çift tık: düzle", window, cx))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                window.focus(&this.focus, cx);
                                if e.click_count >= 2 {
                                    this.canvas_patch_wire(json!({ "bend": null }), window, cx);
                                    return;
                                }
                                if let Some(c) = &mut this.canvas {
                                    c.menu = None;
                                    c.drag = CanvasDrag::Bend { wire: wid.clone(), remembered: false };
                                }
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                );
                let _ = wid2;
            }
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
            // Bağlantı çekilirken hedef şekil vurgu renginde dış hatla belirginleşir.
            let is_target = target_node.as_deref() == Some(n.id.as_str());
            if is_target {
                let grow = 4. + if bw > 0. { bw * zoom } else { 0. };
                let outline = shape_outline(&k, w, h, grow);
                el = el.child(
                    canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
                        stroke_polygon(window, bb.origin, &outline, 2.5, accent);
                    })
                    .absolute()
                    .inset_0(),
                );
            }
            // Bağlantı noktaları (Miro gibi): normalde gizli; şeklin üzerine gelince, seçiliyken ya da bağlantı
            // hedefiyken görünür. Üçgende tepe + yan kenar ortaları + taban ortası. Sürükleme bağlantı çeker.
            if k != "TEXT" && writable && !dragging_nodes {
                // .documents-port: bg-selected zemin, blue-200 kenar; üzerine gelince accent-hover + beyaz.
                let (port_bg, port_border, port_hover) = (p.bg_selected, p.blue_200, p.accent_hover);
                let gid = SharedString::from(format!("node-{}", n.id));
                let shown = (selected && single) || is_target;
                for side in Side::ALL {
                    let q = port_point(n, side);
                    let (cxp, cyp) = ((q.x - n.x as f32) * zoom, (q.y - n.y as f32) * zoom);
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
                            .hover(move |s| s.bg(port_hover).border_color(gpui_kit::white()).opacity(1.))
                            .opacity(if shown { 1. } else { 0. })
                            .when(!connecting, |s| s.group_hover(gid.clone(), |s| s.opacity(1.)))
                            .cursor_crosshair()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                                    cx.stop_propagation();
                                    window.focus(&this.focus, cx);
                                    this.commit_text(window, cx);
                                    if !this.doc.as_ref().is_some_and(|d| d.writable()) {
                                        return;
                                    }
                                    let Some(c) = &mut this.canvas else { return };
                                    if e.modifiers.alt {
                                        // Alt + tık: bu noktadaki bağlantıları sil.
                                        c.remember();
                                        let s = side.name();
                                        c.content.connections.retain(|w| !(w.from == nid && wire_str(w, "fromAnchor") != Some("auto") && wire_str(w, "fromSide") == Some(s)) && !(w.to == nid && wire_str(w, "toAnchor") != Some("auto") && wire_str(w, "toSide") == Some(s)));
                                        this.schedule_save(window, cx);
                                        return;
                                    }
                                    let at = c.node(&nid).map(|n| port_point(n, side)).unwrap_or(pt(0., 0.));
                                    c.drag = CanvasDrag::Connect { from: End::Node { id: nid.clone(), anchor: Anchor::Side(side) }, start: at, at, snap: None };
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
            // Hızlı oluşturma (Miro'daki kenar okları): tek seçili şeklin dört yanında "+"; tıklayınca o yöne
            // aynı biçimde yeni şekil ekler ve bağlar.
            if single && selected && writable && !dragging_nodes && !connecting && k != "TEXT" && k != "IMAGE" && editing.is_none() {
                for side in Side::ALL {
                    let (lx, ly) = match side {
                        Side::Top => (w / 2., -30.),
                        Side::Right => (w + 30., h / 2.),
                        Side::Bottom => (w / 2., h + 30.),
                        Side::Left => (-30., h / 2.),
                    };
                    let nid = n.id.clone();
                    let (hbg, hfg) = (p.accent_hover, gpui_kit::white());
                    el = el.child(
                        div()
                            .id(SharedString::from(format!("quick-{}-{}", n.id, side.name())))
                            .absolute()
                            .left(px(lx - 9.))
                            .top(px(ly - 9.))
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(p.border_strong)
                            .bg(p.bg_2)
                            .text_size(px(13.))
                            .line_height(px(13.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_accent)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hbg).text_color(hfg).border_color(hbg))
                            .tooltip(|window, cx| p2_ui::controls::tooltip("Bu yöne bağlı şekil ekle", window, cx))
                            .child("+")
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| this.canvas_quick_create(&nid, side, window, cx))),
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
                // "+" hızlı oluşturma düğmelerine yer kalsın diye panel şekilden biraz daha uzakta.
                let right = (n.x + n.w) as f32 + 14. + 30. / zoom;
                let left_c = n.x as f32 - 204. / zoom - 30. / zoom;
                let left = if right + 190. / zoom <= view.x + vw || left_c < view.x { right } else { left_c };
                (sx(left), sy(n.y as f32), n)
            });
        let format_panel = format.map(|(fx, fy, n)| self.format_panel(&n, fx, fy, p, cx));
        let Some(c) = &self.canvas else { return div().into_any_element() };
        // Bağlantı araç çubuğu: seçili bağlantının orta noktasının altında (yer yoksa üstünde).
        let wire_panel = c
            .selected_wire
            .clone()
            .filter(|_| writable && c.editing_wire.is_none() && c.menu.is_none() && matches!(c.drag, CanvasDrag::None))
            .and_then(|id| Some((c.content.connections.iter().find(|w| w.id == id).cloned()?, wires.iter().find(|w| w.id == id).map(|w| (polyline_center(&w.path), w.path.iter().map(|q| q.y).fold(f32::MAX, f32::min), w.path.iter().map(|q| q.y).fold(f32::MIN, f32::max)))?)))
            .map(|(w, (m, y0, y1))| self.wire_toolbar(&w, (sx(m.x), sy(y0), sy(y1)), (f32::from(b.size.width), f32::from(b.size.height)), p, cx));
        let hovering_wire = c.hover_wire.is_some() && matches!(c.drag, CanvasDrag::None);

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
            .when(hovering_wire, |s| s.cursor_pointer())
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| this.canvas_key(e, window, cx)))
            .child(layer)
            .children(labels)
            .children(node_els)
            .children(marquee)
            .children(format_panel)
            .children(wire_handles)
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
        let (can_undo, can_redo) = self.canvas.as_ref().map(|c| (writable && !c.undo.is_empty(), writable && !c.redo.is_empty())).unwrap_or_default();
        let undo_icon = div().text_size(fs(15.)).text_color(p.text_2).child("↶").into_any_element();
        let redo_icon = div().text_size(fs(15.)).text_color(p.text_2).child("↷").into_any_element();
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
                    .child(btn("tool-box", "Kutu", shape(false), false).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, _| this.begin_place("BOX"))).on_click(cx.listener(|this, _, window, cx| this.canvas_add("BOX", None, window, cx))))
                    .child(btn("tool-sphere", "Küre", shape(true), false).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, _| this.begin_place("SPHERE"))).on_click(cx.listener(|this, _, window, cx| this.canvas_add("SPHERE", None, window, cx))))
                    .child(btn("tool-tri", "Üçgen", tri, false).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, _| this.begin_place("TRIANGLE"))).on_click(cx.listener(|this, _, window, cx| this.canvas_add("TRIANGLE", None, window, cx))))
                    .child(btn("tool-text", "Düz metin", t_icon, false).on_click(cx.listener(|this, _, window, cx| this.canvas_add("TEXT", None, window, cx))))
                    .child(btn("tool-file", "Dosya veya medya", image, false).on_click(cx.listener(|this, _, window, cx| this.canvas_pick_file(window, cx))))
                    .child(div().w(px(26.)).h(px(1.)).my(px(3.)).bg(p.border_subtle))
                    .child(btn("tool-grid", grid_title, grid_icon, grid != "NONE").on_click(cx.listener(|this, _, window, cx| this.canvas_cycle_grid(window, cx))))
                    // Geri al / yinele (Ctrl+Z / Ctrl+Y ile aynı geçmiş).
                    .child(div().w(px(26.)).h(px(1.)).my(px(3.)).bg(p.border_subtle))
                    .child(btn("tool-undo", "Geri al (Ctrl+Z)", undo_icon, false).when(!can_undo, |s| s.opacity(0.45)).on_click(cx.listener(|this, _, window, cx| this.canvas_undo(false, window, cx))))
                    .child(btn("tool-redo", "Yinele (Ctrl+Y)", redo_icon, false).when(!can_redo, |s| s.opacity(0.45)).on_click(cx.listener(|this, _, window, cx| this.canvas_undo(true, window, cx)))),
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

    /// Bağlantı araç çubuğu (`.documents-wire-format` görünümü, biçim paneliyle aynı cam yüzey): biçim
    /// (düz / dirsekli / eğri), oklar, çizgi türü, renk, kalınlık, etiket, araya şekil ekle, sil.
    fn wire_toolbar(&self, w: &CanvasWire, mid: (f32, f32, f32), area: (f32, f32), p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let thick = wire_thickness(w) as f64;
        let shape = wire_shape(w);
        let (a_start, a_end) = wire_arrows(w);
        let dashed = wire_dashed(w);
        let color = wire_str(w, "color").unwrap_or("#4f9dff").to_string();
        let (hb, hbg, hs) = (p.border_accent, p.accent_soft, p.text_strong);
        let tbtn = |id: &'static str, title: &'static str, icon: AnyElement, active: bool| {
            div()
                .id(id)
                .size(px(27.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .border_1()
                .border_color(if active { p.border_accent } else { p.border })
                .bg(if active { p.accent_soft } else { p.bg_3 })
                .text_color(if active { p.text_strong } else { p.text })
                .text_size(fs(12.))
                .cursor_pointer()
                .hover(move |s| s.border_color(hb).bg(hbg).text_color(hs))
                .tooltip(move |window, cx| p2_ui::controls::tooltip(title, window, cx))
                .child(icon)
        };
        let ink = p.text;
        let glyph = |pts: Vec<(f32, f32)>, curve: bool, dash: bool| {
            canvas(|_, _, _| {}, move |bb: Bounds<Pixels>, _, window, _| {
                let o = bb.origin;
                let mut b = PathBuilder::stroke(px(1.6));
                if dash {
                    b = b.dash_array(&[px(3.), px(2.5)]);
                }
                b.move_to(o + point(px(pts[0].0), px(pts[0].1)));
                if curve {
                    b.cubic_bezier_to(o + point(px(pts[3].0), px(pts[3].1)), o + point(px(pts[1].0), px(pts[1].1)), o + point(px(pts[2].0), px(pts[2].1)));
                } else {
                    for q in &pts[1..] {
                        b.line_to(o + point(px(q.0), px(q.1)));
                    }
                }
                if let Ok(path) = b.build() {
                    window.paint_path(path, ink);
                }
            })
            .size(px(16.))
            .into_any_element()
        };
        let text = |s: &'static str| div().child(s).into_any_element();
        fn set(cx: &mut Context<DocumentsPage>, patch: Value) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static {
            cx.listener(move |this, _, window, cx| this.canvas_patch_wire(patch.clone(), window, cx))
        }
        let sep = || div().w(px(1.)).h(px(20.)).mx(px(2.)).bg(p.border_subtle);
        let row1 = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(tbtn("wt-straight", "Düz", glyph(vec![(2., 14.), (14., 2.)], false, false), shape == WireShape::Straight).on_click(set(cx, json!({ "shape": "straight" }))))
            .child(tbtn("wt-elbow", "Dirsekli", glyph(vec![(2., 13.), (8., 13.), (8., 3.), (14., 3.)], false, false), shape == WireShape::Elbow).on_click(set(cx, json!({ "shape": "elbow" }))))
            .child(tbtn("wt-curved", "Eğri", glyph(vec![(2., 14.), (10., 14.), (6., 2.), (14., 2.)], true, false), shape == WireShape::Curved).on_click(set(cx, json!({ "shape": "curved" }))))
            .child(sep())
            .child(tbtn("wt-a-none", "Ok yok", text("—"), !a_start && !a_end).on_click(set(cx, json!({ "arrow": "none" }))))
            .child(tbtn("wt-a-start", "Başta ok", text("←"), a_start && !a_end).on_click(set(cx, json!({ "arrow": "start" }))))
            .child(tbtn("wt-a-end", "Sonda ok", text("→"), !a_start && a_end).on_click(set(cx, json!({ "arrow": "end" }))))
            .child(tbtn("wt-a-both", "İki uçta ok", text("↔"), a_start && a_end).on_click(set(cx, json!({ "arrow": "both" }))))
            .child(sep())
            .child(tbtn("wt-dash", if dashed { "Düz çizgi" } else { "Kesik çizgi" }, glyph(vec![(1., 8.), (15., 8.)], false, true), dashed).on_click(set(cx, json!({ "lineStyle": if dashed { "solid" } else { "dashed" } }))));
        let mut swatches = div().flex().items_center().gap(px(4.));
        for col in WIRE_SWATCHES {
            let ring = p.accent_hover;
            let active = color.eq_ignore_ascii_case(col);
            swatches = swatches.child(
                div()
                    .id(SharedString::from(format!("wsw-{col}")))
                    .size(px(17.))
                    .rounded_full()
                    .border_2()
                    .border_color(p.bg_2)
                    .bg(css_color(Some(col), gpui_kit::black()))
                    .shadow(vec![gpui_kit::BoxShadow { color: if active { ring } else { p.border }, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(if active { 2. } else { 1. }), inset: false }])
                    .cursor_pointer()
                    .hover(move |s| s.shadow(vec![gpui_kit::BoxShadow { color: ring, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(2.), inset: false }]))
                    .on_click(cx.listener(move |this, _, window, cx| this.canvas_patch_wire(json!({ "color": col }), window, cx))),
            );
        }
        let wid = w.id.clone();
        let wid2 = w.id.clone();
        let row2 = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(swatches)
            .child(sep())
            .child(tbtn("wt-", "İncelt", text("−"), false).on_click(set(cx, json!({ "thickness": (thick - 0.5).max(1.) }))))
            .child(div().w(px(34.)).text_center().text_size(fs(9.)).text_color(p.text).child(format!("{thick:.1}px")))
            .child(tbtn("wt+", "Kalınlaştır", text("+"), false).on_click(set(cx, json!({ "thickness": (thick + 0.5).min(10.) }))));
        let row3 = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(tbtn("wt-label", "Etiket (çift tık)", text("✎"), wire_str(w, "label").is_some()).on_click(cx.listener(move |this, _, window, cx| this.begin_wire_label(wid.clone(), window, cx))))
            .child(tbtn("wt-insert", "Araya şekil ekle", text("⊕"), false).on_click(cx.listener(move |this, _, window, cx| this.canvas_insert_on_wire(&wid2, window, cx))))
            .child(div().flex_1())
            .child(tbtn("wt-del", "Bağlantıyı sil (Delete)", text("🗑"), false).text_color(p.danger_text).on_click(cx.listener(|this, _, window, cx| this.canvas_remove_selected(window, cx))));
        let (pw, ph) = (322., 128.);
        let left = (mid.0 - pw / 2.).clamp(8., (area.0 - pw - 8.).max(8.));
        // Miro gibi seçimin üstünde; yer yoksa altında.
        let top = if mid.1 - 22. - ph >= 8. { mid.1 - 22. - ph } else { (mid.2 + 22.).min(area.1 - ph - 8.).max(8.) };
        div()
            .id("doc-wire-format")
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(pw))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .rounded(radius::MD)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background())
            .backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("doc-wire-format", Entrance::ScaleIn, Duration::ZERO)
            .child(div().text_size(font_size::XXS).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child("WİRE AYARLARI"))
            .child(row1)
            .child(row2)
            .child(row3)
            .into_any_element()
    }

    /// Tuval sağ tık menüleri (MessageMenuPopup görünümü).
    pub(super) fn canvas_menu_layer(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.canvas.as_ref()?;
        let menu = c.menu.clone()?;
        let tall = matches!(menu, CanvasMenu::Wire { .. });
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
            CanvasMenu::Wire { id, at } => {
                // Bağlantı menüsü: biçim, oklar, çizgi türü, etiket, araya ekle, sil.
                let w = c.content.connections.iter().find(|w| w.id == id)?.clone();
                let (shape, (a_s, a_e), dashed) = (wire_shape(&w), wire_arrows(&w), wire_dashed(&w));
                let tick = |on: bool| Some(div().w(px(16.)).child(if on { "✓" } else { "" }).into_any_element());
                fn set(cx: &mut Context<DocumentsPage>, patch: Value) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static {
                    cx.listener(move |this, _, window, cx| this.canvas_patch_wire(patch.clone(), window, cx))
                }
                let mut m = p2_ui::controls::menu("canvas-wire-menu", p).w(px(230.));
                if writable {
                    let (w1, w2) = (id.clone(), id.clone());
                    m = m
                        .child(p2_ui::controls::menu_label("BİÇİM", p))
                        .child(p2_ui::controls::menu_item("wm-straight", "Düz", tick(shape == WireShape::Straight), None, false, p).on_click(set(cx, json!({ "shape": "straight" }))))
                        .child(p2_ui::controls::menu_item("wm-elbow", "Dirsekli", tick(shape == WireShape::Elbow), None, false, p).on_click(set(cx, json!({ "shape": "elbow" }))))
                        .child(p2_ui::controls::menu_item("wm-curved", "Eğri", tick(shape == WireShape::Curved), None, false, p).on_click(set(cx, json!({ "shape": "curved" }))))
                        .child(p2_ui::controls::menu_separator(p))
                        .child(p2_ui::controls::menu_label("OKLAR", p))
                        .child(p2_ui::controls::menu_item("wm-a-none", "Ok yok", tick(!a_s && !a_e), None, false, p).on_click(set(cx, json!({ "arrow": "none" }))))
                        .child(p2_ui::controls::menu_item("wm-a-start", "Başta ok", tick(a_s && !a_e), None, false, p).on_click(set(cx, json!({ "arrow": "start" }))))
                        .child(p2_ui::controls::menu_item("wm-a-end", "Sonda ok", tick(!a_s && a_e), None, false, p).on_click(set(cx, json!({ "arrow": "end" }))))
                        .child(p2_ui::controls::menu_item("wm-a-both", "İki uçta ok", tick(a_s && a_e), None, false, p).on_click(set(cx, json!({ "arrow": "both" }))))
                        .child(p2_ui::controls::menu_separator(p))
                        .child(p2_ui::controls::menu_item("wm-dash", "Kesik çizgi", tick(dashed), None, false, p).on_click(set(cx, json!({ "lineStyle": if dashed { "solid" } else { "dashed" } }))))
                        .child(p2_ui::controls::menu_item("wm-label", "Etiketi düzenle", Some(div().w(px(16.)).child("✎").into_any_element()), None, false, p).on_click(cx.listener(move |this, _, window, cx| this.begin_wire_label(w1.clone(), window, cx))))
                        .child(p2_ui::controls::menu_item("wm-insert", "Araya şekil ekle", Some(div().w(px(16.)).child("⊕").into_any_element()), None, false, p).on_click(cx.listener(move |this, _, window, cx| this.canvas_insert_on_wire(&w2, window, cx))))
                        .child(p2_ui::controls::menu_separator(p))
                        .child(p2_ui::controls::menu_item("wm-del", "Bağlantıyı sil", Some(div().w(px(16.)).child("🗑").into_any_element()), Some("Delete".into()), true, p).on_click(cx.listener(|this, _, window, cx| {
                            if let Some(c) = &mut this.canvas {
                                c.menu = None;
                            }
                            this.canvas_remove_selected(window, cx);
                        })));
                } else {
                    m = m.child(p2_ui::controls::menu_label("BAĞLANTI · SALT OKUNUR", p));
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
        let y = f32::from(at.y).min(f32::from(vp.height) - if tall { 450. } else { 260. }).max(8.);
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

#[cfg(test)]
mod wire_tests {
    use super::*;

    fn node(id: &str, kind_: &str, x: f64, y: f64, w: f64, h: f64) -> CanvasNode {
        CanvasNode { id: id.into(), kind: kind_.into(), x, y, w, h, text: None, color: None, extra: Map::new() }
    }
    fn wire(v: Value) -> CanvasWire {
        serde_json::from_value(v).unwrap()
    }
    fn near(a: P, b: P) -> bool {
        (a.x - b.x).abs() < 0.6 && (a.y - b.y).abs() < 0.6
    }
    /// Yolun hiçbir parçası (uçlar hariç) düğümün içinden geçmez.
    fn avoids(r: &[P], n: &CanvasNode) -> bool {
        r.windows(2).all(|w| {
            (1..20).all(|i| {
                let t = i as f32 / 20.;
                let q = pt(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t);
                !shape_contains(kind(n), n.w as f32, n.h as f32, q.x - n.x as f32, q.y - n.y as f32, -2.)
            })
        })
    }
    fn orthogonal(r: &[P]) -> bool {
        r.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-3 || (w[0].y - w[1].y).abs() < 1e-3)
    }

    #[test]
    fn ucgen_baglanti_noktalari_hat_uzerinde() {
        let t = node("t", "TRIANGLE", 0., 0., 210., 180.);
        assert!(near(port_point(&t, Side::Top), pt(105., 0.)), "tepe");
        assert!(near(port_point(&t, Side::Left), pt(52.5, 90.)), "sol kenar ortası");
        assert!(near(port_point(&t, Side::Right), pt(157.5, 90.)), "sağ kenar ortası");
        assert!(near(port_point(&t, Side::Bottom), pt(105., 180.)), "taban ortası");
        for s in Side::ALL {
            let q = port_point(&t, s);
            assert!(shape_contains("TRIANGLE", 210., 180., q.x, q.y, 0.5), "{s:?} hat üzerinde");
            assert!(!shape_contains("TRIANGLE", 210., 180., q.x, q.y, -1.), "{s:?} içeride değil");
        }
    }

    #[test]
    fn dis_hat_kesisimi_sekli_izler() {
        let b = node("a", "BOX", 0., 0., 200., 110.);
        assert!(near(outline_toward(&b, pt(500., 55.)), pt(200., 55.)));
        assert!(near(outline_toward(&b, pt(100., -300.)), pt(100., 0.)));
        let s = node("s", "SPHERE", 0., 0., 160., 160.);
        let q = outline_toward(&s, pt(1000., 1000.));
        // 45 derece: (80 + 80 cos45, 80 + 80 sin45) ~ (136.6, 136.6); çokgen yaklaşımıyla +-1.
        assert!((q.x - 136.57).abs() < 1.2 && (q.y - 136.57).abs() < 1.2, "{q:?}");
        let t = node("t", "TRIANGLE", 0., 0., 210., 180.);
        let q = outline_toward(&t, pt(-500., 120.));
        assert!(shape_contains("TRIANGLE", 210., 180., q.x, q.y, 0.5) && !shape_contains("TRIANGLE", 210., 180., q.x, q.y, -1.), "üçgen sol kenarında: {q:?}");
    }

    #[test]
    fn eski_bicim_okunur_ve_bilinmeyen_alanlar_korunur() {
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left", "label": "akış", "points": [] }));
        assert_eq!(wire_end(&w, Which::From), End::Node { id: "a".into(), anchor: Anchor::Side(Side::Right) });
        assert_eq!(wire_shape(&w), WireShape::Elbow, "eski bağlantı dirsekli kalır");
        assert_eq!(wire_arrows(&w), (false, true), "eski bağlantı sonda oklu");
        let content = CanvasContent::from_value(&json!({ "version": 1, "nodes": [], "connections": [
            { "id": "w2", "from": "a", "to": "", "toPoint": { "x": 5, "y": 6 }, "shape": "curved", "arrow": "both", "lineStyle": "dashed", "gelecek": 1 }
        ] }));
        let w2 = &content.connections[0];
        assert_eq!(wire_end(w2, Which::To), End::Free(pt(5., 6.)));
        assert_eq!(wire_shape(w2), WireShape::Curved);
        assert!(wire_dashed(w2));
        let back = content.to_value();
        assert_eq!(back["connections"][0]["gelecek"], json!(1), "bilinmeyen alan korunur");
        assert_eq!(back["connections"][0]["to"], json!(""), "eski okuyucular için `to` alanı hep var");
    }

    #[test]
    fn uc_yazma_ilgisiz_alanlari_temizler() {
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "BOX", 420., 0., 200., 110.)];
        let mut w = make_wire("w".into(), &End::Node { id: "a".into(), anchor: Anchor::Side(Side::Right) }, &End::Free(pt(300., 300.)), WireShape::Straight, &nodes);
        assert_eq!(w.to, "");
        assert!(w.extra.contains_key("toPoint") && !w.extra.contains_key("toSide"));
        store_end(&mut w, Which::To, &End::Node { id: "b".into(), anchor: Anchor::Auto });
        sync_wire_sides(&nodes, &mut w);
        assert_eq!(w.to, "b");
        assert!(!w.extra.contains_key("toPoint"));
        assert_eq!(wire_str(&w, "toAnchor"), Some("auto"));
        assert_eq!(wire_str(&w, "toSide"), Some("left"), "auto uçta eski okuyucular için bakan taraf yazılır");
    }

    #[test]
    fn duz_baglanti_dis_hatta_biter_ve_kendi_seklini_kesmez() {
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "SPHERE", 440., 250., 160., 160.)];
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromAnchor": "auto", "toAnchor": "auto", "shape": "straight" }));
        let r = wire_path(&nodes, &w, &[]).unwrap();
        assert_eq!(r.len(), 2);
        // Uçlar merkezde değil, dış hatta.
        let (sa, sb) = (r[0], r[1]);
        assert!(shape_contains("BOX", 200., 110., sa.x, sa.y, 0.6) && !shape_contains("BOX", 200., 110., sa.x, sa.y, -1.));
        assert!(shape_contains("SPHERE", 160., 160., sb.x - 440., sb.y - 250., 1.5) && !shape_contains("SPHERE", 160., 160., sb.x - 440., sb.y - 250., -2.));
        // Sabit sağ nokta ama hedef solda: uç dış hatta kayar, çizgi kutunun içinden geçmez.
        let nodes = vec![node("a", "BOX", 400., 0., 200., 110.), node("b", "BOX", 0., 0., 200., 110.)];
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left", "shape": "straight" }));
        let r = wire_path(&nodes, &w, &[]).unwrap();
        assert!(avoids(&r, &nodes[0]), "{r:?}");
    }

    #[test]
    fn dirsekli_yol_dik_acili_ve_sekillerin_icinden_gecmez() {
        let a = node("a", "BOX", 0., 0., 200., 110.);
        let b = node("b", "BOX", -300., 300., 200., 110.);
        let nodes = vec![a.clone(), b.clone()];
        // Sağdan çıkıp sol aşağıdaki kutunun üstüne: kutuların etrafından dolaşmalı.
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "top" }));
        let r = wire_path(&nodes, &w, &[]).unwrap();
        assert!(orthogonal(&r), "{r:?}");
        assert!(near(r[0], pt(200., 55.)) && near(*r.last().unwrap(), pt(-200., 300.)), "{r:?}");
        assert!(avoids(&r, &a) && avoids(&r, &b), "{r:?}");
        // Üçgenin yan noktasından dirsek: önce sınır kutusundan çıkar, üçgeni kesmez.
        let t = node("t", "TRIANGLE", 0., 240., 210., 180.);
        let s = node("s", "SPHERE", 440., 250., 160., 160.);
        let nodes = vec![t.clone(), s.clone()];
        let w = wire(json!({ "id": "w", "from": "t", "to": "s", "fromSide": "left", "toSide": "left" }));
        let r = wire_path(&nodes, &w, &[]).unwrap();
        assert!(orthogonal(&r) && avoids(&r, &t) && avoids(&r, &s), "{r:?}");
        assert!(near(r[0], port_point(&t, Side::Left)));
    }

    #[test]
    fn bukulme_noktasi_orta_parcayi_tasir() {
        let a = Tip { p: pt(200., 55.), side: Some(Side::Right), gap: 18. };
        let b = Tip { p: pt(420., 300.), side: Some(Side::Left), gap: 18. };
        let r = elbow_bend(a, b, pt(320., 150.));
        assert!(orthogonal(&r));
        assert!(r.iter().filter(|q| (q.x - 320.).abs() < 1e-3).count() >= 2, "dikey orta parça x=320: {r:?}");
        let c = curve(a, b, Some(pt(250., 250.)));
        assert!(near(c[0], a.p) && near(*c.last().unwrap(), b.p));
        assert!(c.iter().any(|q| near(*q, pt(250., 250.))), "eğri bükülme noktasından geçer");
    }

    #[test]
    fn yol_isabet_testi_toleransli() {
        let r = vec![pt(0., 0.), pt(100., 0.), pt(100., 100.)];
        assert!(dist_to_polyline(&r, pt(50., 4.)) <= 7.);
        assert!(dist_to_polyline(&r, pt(104., 60.)) <= 7.);
        assert!(dist_to_polyline(&r, pt(50., 30.)) > 7.);
        assert!((dist_to_polyline(&r, pt(-3., -4.)) - 5.).abs() < 1e-4, "uçtan öte: uca uzaklık");
        let n = node("n", "BOX", 40., -20., 30., 40.);
        assert!(path_hits_node(&r, &n));
        let far = node("f", "BOX", 20., 20., 30., 30.);
        assert!(!path_hits_node(&r, &far));
        let tri = node("t", "TRIANGLE", 0., -10., 40., 9.);
        assert!(!path_hits_node(&r, &tri), "üçgenin sınır kutusu değil, şekli");
    }

    #[test]
    fn birakma_yapisma_noktasi_ic_ve_bos() {
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("t", "TEXT", 300., 0., 100., 30.)];
        assert_eq!(snap_end(&nodes, pt(203., 57.), &[], 1.), Some(End::Node { id: "a".into(), anchor: Anchor::Side(Side::Right) }));
        assert_eq!(snap_end(&nodes, pt(100., 60.), &[], 1.), Some(End::Node { id: "a".into(), anchor: Anchor::Auto }));
        assert_eq!(snap_end(&nodes, pt(600., 600.), &[], 1.), None, "boşa bırakılan uç serbest");
        assert_eq!(snap_end(&nodes, pt(320., 10.), &[], 1.), None, "düz metne bağlanılmaz");
        assert_eq!(snap_end(&nodes, pt(100., 60.), &["a"], 1.), None, "kaynağın kendisi dışlanır");
    }

    #[test]
    fn araya_ekleme_baglantiyi_boler_ve_bicimi_korur() {
        let mut content = CanvasContent::from_value(&json!({ "version": 2, "nodes": [
            { "id": "a", "type": "BOX", "x": 0, "y": 0, "w": 200, "h": 110 },
            { "id": "b", "type": "BOX", "x": 600, "y": 0, "w": 200, "h": 110 },
            { "id": "n", "type": "SPHERE", "x": 330, "y": -30, "w": 160, "h": 160 }
        ], "connections": [
            { "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left", "color": "#7d3643", "thickness": 3.5, "arrow": "both", "lineStyle": "dashed", "shape": "curved", "label": "akış", "bend": { "x": 1, "y": 2 } }
        ] }));
        assert!(!split_wire(&mut content, "w", "a", "w2".into()), "uç düğümle bölünmez");
        assert!(split_wire(&mut content, "w", "n", "w2".into()));
        assert_eq!(content.connections.len(), 2);
        let (first, second) = (&content.connections[0], &content.connections[1]);
        assert_eq!((first.from.as_str(), first.to.as_str()), ("a", "n"));
        assert_eq!((second.from.as_str(), second.to.as_str()), ("n", "b"));
        assert_eq!(wire_end(first, Which::From), End::Node { id: "a".into(), anchor: Anchor::Side(Side::Right) }, "dış uçlar korunur");
        assert_eq!(wire_end(second, Which::To), End::Node { id: "b".into(), anchor: Anchor::Side(Side::Left) });
        assert_eq!(wire_end(first, Which::To), End::Node { id: "n".into(), anchor: Anchor::Auto });
        assert_eq!(wire_str(first, "toSide"), Some("left"), "yeni düğüm A'ya solundan bakar");
        assert_eq!(wire_str(second, "fromSide"), Some("right"));
        for w in [first, second] {
            assert_eq!(wire_str(w, "color"), Some("#7d3643"));
            assert_eq!(wire_thickness(w), 3.5);
            assert_eq!(wire_arrows(w), (true, true));
            assert!(wire_dashed(w));
            assert_eq!(wire_shape(w), WireShape::Curved);
            assert!(!w.extra.contains_key("bend"), "bükülme sıfırlanır");
        }
        assert_eq!(wire_str(first, "label"), Some("akış"));
        assert!(wire_str(second, "label").is_none(), "etiket tek parçada kalır");
        assert!(wire_path(&content.nodes, first, &[]).is_some() && wire_path(&content.nodes, second, &[]).is_some());
    }

    #[test]
    fn surukleme_sirasinda_yol_suruklenen_dugumu_yok_sayar() {
        // A->B dirsekli bağlantı; araya C sürükleniyor. C engel sayılırsa yol kaçar ve araya ekleme hiç tutmaz.
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "BOX", 600., 0., 200., 110.), node("c", "BOX", 330., 20., 100., 70.)];
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left" }));
        let avoid = wire_path(&nodes, &w, &[]).unwrap();
        assert!(!path_hits_node(&avoid, &nodes[2]), "normalde C'nin etrafından dolaşır");
        let through = wire_path(&nodes, &w, &["c"]).unwrap();
        assert!(path_hits_node(&through, &nodes[2]), "sürüklenirken düz geçer: {through:?}");
        // Kendi uçları yok sayılamaz.
        let own = wire_path(&nodes, &w, &["a", "b"]).unwrap();
        assert!(avoids(&own, &nodes[0]) && avoids(&own, &nodes[1]));
    }

    #[test]
    fn ok_basi_icin_yol_kirpilir() {
        let r = vec![pt(0., 0.), pt(10., 0.), pt(10., 50.)];
        let t = trim_polyline(&r, 4., 10.);
        assert!(near(t[0], pt(4., 0.)) && near(*t.last().unwrap(), pt(10., 40.)), "{t:?}");
        assert_eq!(behind(&r).map(|(q, tip)| (q.y, tip.y)), Some((0., 50.)));
    }

    #[test]
    fn bukulme_geri_donmez() {
        // Sağdan çıkan uç; bükülme noktası şeklin arkasında (solda): yatay orta parçaya geçilir, şekle geri girilmez.
        let a = Tip { p: pt(200., 55.), side: Some(Side::Right), gap: 18. };
        let b = Tip { p: pt(420., 300.), side: Some(Side::Left), gap: 18. };
        let r = elbow_bend(a, b, pt(0., 380.));
        assert!(orthogonal(&r));
        assert!(r.iter().all(|q| q.x >= 200. - 1e-3 || q.y > 110.), "A'nın içine geri dönmez: {r:?}");
        assert!(r.iter().any(|q| (q.y - 380.).abs() < 1e-3), "yatay orta parça y=380: {r:?}");
    }

    #[test]
    fn yan_yana_seklerde_bukulme_u_yolu_verir() {
        // Aynı hizadaki iki kutu arası düz dirsek; orta tutamaç aşağı çekilince yol tutamaçtan geçer.
        let a = Tip { p: pt(200., 55.), side: Some(Side::Right), gap: 18. };
        let b = Tip { p: pt(420., 55.), side: Some(Side::Left), gap: 18. };
        let r = elbow_bend(a, b, pt(310., 200.));
        assert!(orthogonal(&r));
        assert!(dist_to_polyline(&r, pt(310., 200.)) < 0.5, "{r:?}");
    }

    /// Dirsekli yol kalitesi: dik açılı, hiçbir şeklin içinden geçmez, son parça (ok saplaması) dışında
    /// 10 birimden kısa parça yok.
    fn assert_clean(r: &[P], nodes: &[CanvasNode], what: &str) {
        assert!(orthogonal(r), "{what}: dik açılı değil {r:?}");
        for n in nodes.iter().filter(|n| n.kind != "TEXT") {
            assert!(avoids(r, n), "{what}: {} şeklinin içinden geçiyor {r:?}", n.id);
        }
        for (i, w) in r.windows(2).enumerate() {
            if i + 2 < r.len() {
                assert!(seg_len(w[0], w[1]) >= MIN_SEGMENT - 1e-3, "{what}: {i}. parça kısa ({}) {r:?}", seg_len(w[0], w[1]));
            }
        }
    }

    fn demo2() -> CanvasContent {
        CanvasContent::from_value(&json!({ "version": 2, "nodes": [
            { "id": "a", "type": "BOX", "x": 0, "y": 0, "w": 200, "h": 110 },
            { "id": "b", "type": "BOX", "x": 440, "y": -80, "w": 200, "h": 110 },
            { "id": "c", "type": "SPHERE", "x": 460, "y": 140, "w": 160, "h": 160 },
            { "id": "d", "type": "TRIANGLE", "x": -10, "y": 250, "w": 210, "h": 180 },
            { "id": "e", "type": "BOX", "x": 760, "y": 40, "w": 180, "h": 100 }
        ], "connections": [
            { "id": "e1", "from": "a", "to": "d", "fromSide": "bottom", "toSide": "top", "shape": "elbow" },
            { "id": "e2", "from": "d", "to": "c", "fromSide": "right", "toSide": "bottom", "shape": "elbow", "bend": { "x": 0, "y": 380 } },
            { "id": "e3", "from": "d", "to": "c", "fromSide": "right", "toSide": "bottom", "shape": "elbow" },
            { "id": "e4", "from": "c", "to": "a", "fromSide": "left", "toSide": "right", "shape": "elbow" },
            { "id": "e5", "from": "d", "to": "e", "fromSide": "left", "toSide": "top", "shape": "elbow" },
            { "id": "e6", "from": "a", "to": "b", "fromAnchor": "auto", "toAnchor": "auto", "shape": "elbow" }
        ] }))
    }

    #[test]
    fn yonlendirici_sekillerden_gecmez_ve_zikzak_yapmaz() {
        let content = demo2();
        for w in &content.connections {
            let r = wire_path(&content.nodes, w, &[]).unwrap();
            assert_clean(&r, &content.nodes, &w.id);
        }
        // Kutu altı → üçgen tepesi 5 birim kaymış: zikzak yerine kutu tarafındaki uç kayar, yol düz.
        let e1 = wire_path(&content.nodes, &content.connections[0], &[]).unwrap();
        assert_eq!(e1.len(), 2, "hizalanmış düz iniş: {e1:?}");
        // Üçgenin yan noktasından çıkış saplaması kenar boyunca değil, yatay olarak dışarı.
        let e3 = wire_path(&content.nodes, &content.connections[2], &[]).unwrap();
        assert!((e3[0].y - e3[1].y).abs() < 1e-3 && e3[1].x > e3[0].x, "{e3:?}");
        // Az dönüş: komşu iki şekil arası en çok 3 dönüş.
        assert!(e3.len() <= 5, "fazla dönüş: {e3:?}");
    }

    #[test]
    fn araya_ekleme_sonrasi_yollar_temiz() {
        let mut content = CanvasContent::from_value(&json!({ "version": 2, "nodes": [
            { "id": "a", "type": "BOX", "x": 0, "y": 0, "w": 200, "h": 110 },
            { "id": "b", "type": "BOX", "x": 700, "y": 0, "w": 200, "h": 110 },
            { "id": "t", "type": "TRIANGLE", "x": 245, "y": -35, "w": 210, "h": 180 },
            { "id": "s", "type": "SPHERE", "x": 720, "y": 250, "w": 160, "h": 160 }
        ], "connections": [
            { "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left", "label": "akış" },
            { "id": "x", "from": "s", "to": "a", "fromSide": "right", "toAnchor": "auto", "toSide": "right", "shape": "elbow" }
        ] }));
        assert!(split_wire(&mut content, "w", "t", "w2".into()));
        for w in &content.connections {
            assert!(!w.from.is_empty() && !w.to.is_empty(), "bölme serbest uç üretmez");
            let r = wire_path(&content.nodes, w, &[]).unwrap();
            assert_clean(&r, &content.nodes, &w.id);
        }
    }

    #[test]
    fn paralel_baglantilar_ust_uste_binmez() {
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "BOX", 500., 0., 200., 110.)];
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left" }));
        let r1 = wire_path_lane(&nodes, &w, &[], -7., &[], &[]).unwrap();
        let r2 = wire_path_lane(&nodes, &w, &[], 7., &[], &[]).unwrap();
        assert!((r1[0].y - r2[0].y).abs() >= 13.9, "{r1:?} {r2:?}");
        assert_clean(&r1, &nodes, "r1");
        assert_clean(&r2, &nodes, "r2");
        let s = wire(json!({ "id": "s", "from": "a", "to": "b", "fromAnchor": "auto", "toAnchor": "auto", "shape": "straight" }));
        let back = wire(json!({ "id": "s2", "from": "b", "to": "a", "fromAnchor": "auto", "toAnchor": "auto", "shape": "straight" }));
        let (p1, p2) = (wire_path_lane(&nodes, &s, &[], -7., &[], &[]).unwrap(), wire_path_lane(&nodes, &back, &[], 7., &[], &[]).unwrap());
        assert!(dist_to_polyline(&p1, polyline_center(&p2)) > 10., "ters yönlü düz bağlantılar da ayrışır");
    }

    #[test]
    fn yonlendirici_engelin_etrafindan_dolasir() {
        // A→B arasında duvar: yol duvarın içinden değil etrafından, en çok 4 dönüşle.
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "BOX", 700., 0., 200., 110.), node("wall", "BOX", 380., -200., 60., 500.), node("t", "TRIANGLE", 200., 250., 210., 180.)];
        let w = wire(json!({ "id": "w", "from": "a", "to": "b", "fromSide": "right", "toSide": "left" }));
        let r = wire_path(&nodes, &w, &[]).unwrap();
        assert_clean(&r, &nodes, "duvar");
        assert!(r.len() <= 6, "{r:?}");
        // Hedef kaynağın arkasında: kaynağın kendi şeklinin etrafından döner.
        let w = wire(json!({ "id": "w", "from": "b", "to": "t", "fromSide": "right", "toSide": "left" }));
        let r = wire_path(&nodes, &w, &[]).unwrap();
        assert_clean(&r, &nodes, "arka");
    }

    #[test]
    fn auto_uc_dolu_noktaya_binmez() {
        // A→B, A'nın sağ noktasını kullanıyor; küreden A'ya gelen auto uç başka tarafı seçer.
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "BOX", 560., 0., 200., 110.), node("s", "SPHERE", 440., 250., 160., 160.)];
        let x = wire(json!({ "id": "x", "from": "s", "to": "a", "fromSide": "right", "toAnchor": "auto", "toSide": "right", "shape": "elbow" }));
        let busy = vec![("a".to_string(), Side::Right), ("b".to_string(), Side::Left), ("s".to_string(), Side::Right)];
        let r = wire_path_lane(&nodes, &x, &[], 0., &busy, &[]).unwrap();
        assert!(!near(*r.last().unwrap(), pt(200., 55.)), "dolu sağ noktaya binmedi: {r:?}");
        assert_clean(&r, &nodes, "x");
    }

    #[test]
    fn farkli_baglantilar_ayni_seride_binmez() {
        // İki bağlantı aynı koridoru kullanmak zorunda: ikincisi dolu şeridin yanındaki şeride kayar.
        let nodes = vec![node("a", "BOX", 0., 0., 200., 110.), node("b", "BOX", 600., 0., 200., 110.), node("c", "BOX", 0., 300., 200., 110.), node("d", "BOX", 600., 300., 200., 110.)];
        let w1 = wire(json!({ "id": "1", "from": "a", "to": "d", "fromSide": "bottom", "toSide": "left" }));
        let w2 = wire(json!({ "id": "2", "from": "c", "to": "b", "fromSide": "top", "toSide": "left" }));
        let r1 = wire_path(&nodes, &w1, &[]).unwrap();
        let occ: Vec<(P, P)> = r1.windows(2).map(|s| (s[0], s[1])).collect();
        let r2 = wire_path_lane(&nodes, &w2, &[], 0., &[], &occ).unwrap();
        assert_clean(&r1, &nodes, "1");
        assert_clean(&r2, &nodes, "2");
        for s in r2.windows(2) {
            assert!(overlap_cost(s[0], s[1], &occ) == 0., "üst üste biniyor: {r1:?} / {r2:?}");
        }
    }
}
