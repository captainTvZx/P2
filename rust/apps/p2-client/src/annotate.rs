//! Yayın üzerine kalemle çizim — drawBus.ts + StreamPlayer.tsx çizim yarısı + main.cjs
//! `OVERLAY_HTML`. İzleyenler paylaşılan ekranın üstüne çizer; çizgiler ve kalem imleçleri
//! `call:signal {draw}` / `{cursor}` ile aramadaki herkese gider, kısa süre tam görünür kalır,
//! sonra yavaşça silinir. Yayını yapan kişinin gerçek masaüstünde de saydam, tıklamayı
//! geçiren bir kaplama penceresinde görünür (`overlay_draw.rs`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Bounds, FontWeight, Hsla, PathBuilder, Pixels, Window, canvas, div, point, px, rgb};
use serde_json::{Value, json};

/// Çizgi bu kadar tam görünür kalır…
pub const STROKE_HOLD: Duration = Duration::from_millis(2200);
/// …sonra bu sürede yavaşça silinir.
pub const STROKE_FADE: Duration = Duration::from_millis(2600);
/// İmleç hareketsiz kalırsa bu sürede kaybolur (son 600 ms'de solar).
pub const CURSOR_LIFE: Duration = Duration::from_millis(2600);

/// drawBus.ts `PEN_PALETTE` (seçilebilir renkler).
pub const PEN_PALETTE: [&str; 8] = ["#ff6b6b", "#ffa94d", "#ffd43b", "#69db7c", "#4dabf7", "#da77f2", "#ffffff", "#14181d"];
const PENS: [&str; 10] = ["#ff6b6b", "#ffa94d", "#ffd43b", "#69db7c", "#38d9a9", "#4dabf7", "#748ffc", "#da77f2", "#f783ac", "#63e6be"];

/// Kişi başına sabit kalem rengi (drawBus.ts `penColor`: UTF-16 birimleriyle `hash*31+c >>> 0`).
pub fn pen_color(key: &str) -> &'static str {
    let mut hash: u32 = 0;
    for unit in key.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(unit as u32);
    }
    PENS[(hash % PENS.len() as u32) as usize]
}

/// "#rrggbb" → renk (geçersizse beyaz).
pub fn color(hex: &str) -> Hsla {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).map(|v| rgb(v).into()).unwrap_or_else(|_| gpui_kit::white())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: String,
    pub width: f32,
    /// Yayın görüntüsüne göre 0..1.
    pub points: Vec<(f32, f32)>,
    pub touched: Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    pub x: f32,
    pub y: f32,
    pub name: String,
    pub color: String,
    pub at: Instant,
}

/// Bir yayının (hedef kişi) çizimleri ve imleçleri.
#[derive(Default, Clone, Debug)]
pub struct Board {
    /// Çizgi kimliği → çizgi (gelen parçalar aynı kimliğe eklenir).
    pub strokes: Vec<(String, Stroke)>,
    pub cursors: HashMap<String, Cursor>,
}

impl Board {
    pub fn add_stroke(&mut self, id: &str, color: &str, width: f32, points: &[(f32, f32)], now: Instant) {
        match self.strokes.iter_mut().find(|(sid, _)| sid == id) {
            Some((_, s)) => {
                s.points.extend_from_slice(points);
                s.touched = now;
            }
            None => self.strokes.push((id.to_string(), Stroke { color: color.to_string(), width: if width > 0.0 { width } else { 4.0 }, points: points.to_vec(), touched: now })),
        }
    }

    pub fn set_cursor(&mut self, from: &str, cursor: Option<Cursor>) {
        match cursor {
            Some(c) => {
                self.cursors.insert(from.to_string(), c);
            }
            None => {
                self.cursors.remove(from);
            }
        }
    }

    /// Süresi dolan çizgi ve imleçleri atar; boş kaldıysa true.
    pub fn prune(&mut self, now: Instant) -> bool {
        self.strokes.retain(|(_, s)| now.duration_since(s.touched) <= STROKE_HOLD + STROKE_FADE);
        self.cursors.retain(|_, c| now.duration_since(c.at) <= CURSOR_LIFE);
        self.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.cursors.is_empty()
    }
}

/// Çizginin o anki görünürlüğü (StreamPlayer.tsx: HOLD boyunca 1, sonra FADE'de 0'a).
pub fn stroke_alpha(age: Duration) -> f32 {
    if age < STROKE_HOLD {
        1.0
    } else {
        (1.0 - (age - STROKE_HOLD).as_secs_f32() / STROKE_FADE.as_secs_f32()).max(0.0)
    }
}

/// İmlecin görünürlüğü: son 600 ms'de solar.
pub fn cursor_alpha(age: Duration) -> f32 {
    let life = CURSOR_LIFE.as_secs_f32();
    let a = age.as_secs_f32();
    if a > life - 0.6 { ((life - a) / 0.6).max(0.0) } else { 1.0 }
}

/// `call:signal` verisi: `{draw:{target,id,color,width,points:[{x,y}]}}`.
pub fn draw_payload(target: &str, id: &str, color: &str, width: f32, points: &[(f32, f32)]) -> Value {
    json!({ "draw": { "target": target, "id": id, "color": color, "width": width, "points": points.iter().map(|(x, y)| json!({ "x": x, "y": y })).collect::<Vec<_>>() } })
}

/// `{cursor:{target,name,color,x,y,gone?}}`.
pub fn cursor_payload(target: &str, name: &str, color: &str, x: f32, y: f32, gone: bool) -> Value {
    let mut c = json!({ "target": target, "name": name, "color": color, "x": x, "y": y });
    if gone {
        c["gone"] = json!(true);
    }
    json!({ "cursor": c })
}

pub fn parse_points(v: &Value) -> Vec<(f32, f32)> {
    v.as_array().into_iter().flatten().filter_map(|p| Some((p["x"].as_f64()? as f32, p["y"].as_f64()? as f32))).collect()
}

/// Görüntünün kutu içinde gerçekte kapladığı alan (object-fit: contain).
pub fn content_box(area: Bounds<Pixels>, source: Option<(u32, u32)>) -> Bounds<Pixels> {
    let Some((sw, sh)) = source.filter(|(w, h)| *w > 0 && *h > 0) else { return area };
    let (w, h) = (f32::from(area.size.width), f32::from(area.size.height));
    let scale = (w / sw as f32).min(h / sh as f32);
    let (cw, ch) = (sw as f32 * scale, sh as f32 * scale);
    Bounds { origin: point(area.origin.x + px((w - cw) / 2.0), area.origin.y + px((h - ch) / 2.0)), size: gpui_kit::size(px(cw), px(ch)) }
}

/// Ölçek: kutucuktaki çizim (11 px yazı) ile masaüstü kaplaması (13 px yazı) aynı çizimin
/// iki boyutudur (StreamPlayer.tsx / OVERLAY_HTML).
#[derive(Clone, Copy)]
pub struct Look {
    /// Ok ucu köşeleri (x,y) — tuvaldeki `moveTo/lineTo` değerleri.
    pub arrow: [(f32, f32); 3],
    pub font: f32,
    pub label_h: f32,
    pub label_pad: f32,
    pub label_dx: f32,
    pub label_dy: f32,
}

pub const TILE_LOOK: Look = Look { arrow: [(11., 4.), (4.5, 6.), (3., 12.)], font: 11., label_h: 17., label_pad: 6., label_dx: 12., label_dy: 10. };
pub const DESKTOP_LOOK: Look = Look { arrow: [(13., 5.), (5.5, 7.), (3.5, 14.)], font: 13., label_h: 20., label_pad: 7., label_dx: 14., label_dy: 12. };

/// Çizgiler (tuval). Gölge: altına koyu, biraz kalın bir çizgi (tuvaldeki `shadowBlur` yerine).
pub fn strokes_canvas(board: Board, source: Option<(u32, u32)>) -> impl IntoElement {
    canvas(
        move |_, _, _| (),
        move |bounds, _, window: &mut Window, _| {
            let area = content_box(bounds, source);
            let now = Instant::now();
            let at = |(x, y): (f32, f32)| point(area.origin.x + area.size.width * x, area.origin.y + area.size.height * y);
            for (_, s) in &board.strokes {
                let alpha = stroke_alpha(now.duration_since(s.touched));
                if alpha <= 0.0 || s.points.is_empty() {
                    continue;
                }
                for (extra, c) in [(2.0, gpui_kit::hsla(0., 0., 0., 0.35 * alpha)), (0.0, color(&s.color).opacity(alpha))] {
                    let mut path = PathBuilder::stroke(px(s.width + extra));
                    path.move_to(at(s.points[0]));
                    if s.points.len() == 1 {
                        // Tek nokta: yuvarlak uçlu kısa çizgi gibi görünsün.
                        let (x, y) = s.points[0];
                        path.line_to(point(at((x, y)).x + px(0.5), at((x, y)).y));
                    }
                    for p in &s.points[1..] {
                        path.line_to(at(*p));
                    }
                    if let Ok(built) = path.build() {
                        window.paint_path(built, c);
                    }
                }
            }
            if !board.is_empty() {
                // Solma sürsün: çizim ya da imleç varken her karede yeniden çiz.
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .inset_0()
}

/// Kalem imleçleri: ok ucu + adıyla renkli etiket. `area` yerel koordinatlarda (kutunun içinde).
pub fn cursor_layer(board: &Board, area: (f32, f32, f32, f32), look: Look) -> Vec<AnyElement> {
    let now = Instant::now();
    let (ax, ay, aw, ah) = area;
    let mut out = Vec::new();
    for c in board.cursors.values() {
        let alpha = cursor_alpha(now.duration_since(c.at));
        if alpha <= 0.0 {
            continue;
        }
        let (x, y) = (ax + c.x * aw, ay + c.y * ah);
        let ink = color(&c.color).opacity(alpha);
        let arrow = look.arrow;
        out.push(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .size(px(16.))
                .child(
                    canvas(
                        |_, _, _| (),
                        move |b, _, window: &mut Window, _| {
                            let mut path = PathBuilder::fill();
                            path.move_to(b.origin);
                            for (dx, dy) in arrow {
                                path.line_to(point(b.origin.x + px(dx), b.origin.y + px(dy)));
                            }
                            path.close();
                            if let Ok(p) = path.build() {
                                window.paint_path(p, ink);
                            }
                        },
                    )
                    .size_full(),
                )
                .into_any_element(),
        );
        // Etiket: sağ kenara taşmasın (left = min(x+dx, kutu sağı - genişlik - 14)).
        let approx = c.name.chars().count() as f32 * look.font * 0.58;
        let left = (x + look.label_dx).min(ax + aw - approx - look.label_pad * 2.0 - 2.0);
        out.push(
            div()
                .absolute()
                .left(px(left))
                .top(px(y + look.label_dy))
                .h(px(look.label_h))
                .px(px(look.label_pad))
                .flex()
                .items_center()
                .rounded(px(look.label_h / 2.0))
                .bg(ink)
                .text_size(px(look.font))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color("#14181d").opacity(alpha))
                .whitespace_nowrap()
                .child(c.name.clone())
                .into_any_element(),
        );
    }
    let _ = ah;
    out
}

/// İzleyicinin kalemi: hangi yayında, süren çizgi (kimlik, gönderilmemiş noktalar, son gönderim).
pub struct Pen {
    pub target: String,
    pub drawing: Option<(String, Vec<(f32, f32)>, Instant)>,
    pub pointer_sent: Instant,
    /// Tuvalin son çizim alanı (pencere koordinatları) — fare konumunu 0..1'e çevirmek için.
    pub area: std::rc::Rc<std::cell::Cell<Bounds<Pixels>>>,
}

impl Pen {
    pub fn new(target: &str) -> Self {
        Self { target: target.to_string(), drawing: None, pointer_sent: Instant::now() - Duration::from_secs(1), area: Default::default() }
    }

    /// Pencere konumu → yayına göre 0..1 (kutu dışı da olabilir, tuval gibi).
    pub fn normalize(&self, position: gpui_kit::Point<Pixels>, source: Option<(u32, u32)>) -> (f32, f32) {
        let b = content_box(self.area.get(), source);
        let (w, h) = (f32::from(b.size.width).max(1.0), f32::from(b.size.height).max(1.0));
        (f32::from(position.x - b.origin.x) / w, f32::from(position.y - b.origin.y) / h)
    }
}

/// Yeni çizgi kimliği: `<me ilk 6>-<zaman36>-<rastgele4>` (StreamPlayer.tsx).
pub fn stroke_id(me: &str) -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let rnd = (ms.wrapping_mul(6364136223846793005).wrapping_add(std::process::id() as u64) >> 20) % (36u64.pow(4));
    format!("{}-{}-{}", me.chars().take(6).collect::<String>(), base36(ms), format!("{:0>4}", base36(rnd)))
}

fn base36(mut v: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if v == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while v > 0 {
        s.push(D[(v % 36) as usize]);
        v /= 36;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_color_matches_js_hash() {
        // JS: let h=0; for(c of key) h=(h*31+c.charCodeAt(0))>>>0; PENS[h%10]
        // "a" → 97 → 97%10=7 → #da77f2;  "ab" → 97*31+98=3105 → 5 → #4dabf7
        assert_eq!(pen_color("a"), "#da77f2");
        assert_eq!(pen_color("ab"), "#4dabf7");
        assert_eq!(pen_color(""), "#ff6b6b");
    }

    #[test]
    fn board_merges_chunks_and_fades() {
        let t0 = Instant::now();
        let mut b = Board::default();
        b.add_stroke("s1", "#ff6b6b", 4.0, &[(0.1, 0.1)], t0);
        b.add_stroke("s1", "#ff6b6b", 4.0, &[(0.2, 0.2), (0.3, 0.3)], t0);
        assert_eq!(b.strokes.len(), 1);
        assert_eq!(b.strokes[0].1.points.len(), 3);
        assert_eq!(stroke_alpha(Duration::from_millis(2000)), 1.0);
        assert!((stroke_alpha(STROKE_HOLD + STROKE_FADE / 2) - 0.5).abs() < 0.01);
        assert_eq!(cursor_alpha(Duration::from_millis(100)), 1.0);
        assert!(cursor_alpha(Duration::from_millis(2300)) < 0.6);
        assert!(!b.prune(t0 + Duration::from_millis(4000)));
        assert!(b.prune(t0 + Duration::from_millis(4900)));
    }

    #[test]
    fn payloads_and_box() {
        let d = draw_payload("u", "i", "#fff", 4.0, &[(0.5, 0.25)]);
        assert_eq!(parse_points(&d["draw"]["points"]), vec![(0.5, 0.25)]);
        assert_eq!(cursor_payload("u", "Ad", "#fff", 0.0, 0.0, true)["cursor"]["gone"], json!(true));
        // 16:9 görüntü, 4:3 kutu → üst/alt boşluk.
        let area = Bounds { origin: point(px(0.), px(0.)), size: gpui_kit::size(px(400.), px(300.)) };
        let b = content_box(area, Some((1280, 720)));
        assert_eq!(f32::from(b.size.width), 400.0);
        assert!((f32::from(b.origin.y) - 37.5).abs() < 0.01);
        assert!(stroke_id("abcdefgh").starts_with("abcdef-"));
    }
}
