//! Toasts.tsx — uygulama içi kısa bildirimler (sağ üst, üstten 20px; en çok 3 kart).
//! Her yerden [`push`] ile çağrılır (iş parçacığı güvenli). Ton: `Error` (düz metnin varsayılanı), `Ok`,
//! `Info`; ömür 7 sn (oyunda kartı 9 sn), çıkış `p2-slide-right-out` 360ms ease-in. Kart görünümü
//! `p2_ui::controls::toast` (ui.css `.p2-toast`).
//! Parite kancası (yalnız görünüm): `P2_AUTOPILOT_TOAST=info,ok,error` → DevShowcase'deki örnek kartlar.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, SharedString, Window, div, px};
use p2_ui::controls::{BtnSize, BtnVariant, ToastTone, button, toast};
use p2_ui::tokens::Palette;

use crate::app::ClientApp;

const OUT_MS: u64 = 360;

#[derive(Clone)]
struct Item {
    id: u64,
    tone: ToastTone,
    title: Option<String>,
    text: String,
    life: Duration,
    born: Instant,
    leaving: Option<Instant>,
}

static ITEMS: Mutex<(u64, Vec<Item>)> = Mutex::new((0, Vec::new()));

/// Toasts.tsx `pushToast({text, tone, title})`.
pub fn push(tone: ToastTone, title: Option<&str>, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let mut guard = ITEMS.lock().unwrap();
    guard.0 += 1;
    let id = guard.0;
    let list = &mut guard.1;
    // `[...list.slice(-2), yeni]`
    while list.len() > 2 {
        list.remove(0);
    }
    list.push(Item { id, tone, title: title.map(str::to_string), text: text.to_string(), life: Duration::from_millis(7000), born: Instant::now(), leaving: None });
}

fn dismiss(id: u64) {
    if let Some(item) = ITEMS.lock().unwrap().1.iter_mut().find(|i| i.id == id && i.leaving.is_none()) {
        item.leaving = Some(Instant::now());
    }
}

/// Parite/gösterim: DevShowcase'deki örnek metinler.
pub fn autopilot() {
    let Ok(raw) = std::env::var("P2_AUTOPILOT_TOAST") else { return };
    for tone in raw.split(',') {
        match tone {
            "info" => push(ToastTone::Info, Some("Yeni mesaj"), "Cem Akkaya #genel kanalına yazdı"),
            "ok" => push(ToastTone::Ok, Some("Sürüm yayınlandı"), "MineTastic 1.4 canlıya alındı"),
            "error" => push(ToastTone::Error, Some("Yükleme başarısız"), "Sunucuya ulaşılamadı, yeniden dene"),
            _ => {}
        }
    }
}

impl ClientApp {
    pub(crate) fn toast_stack(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let now = Instant::now();
        let items = {
            let mut guard = ITEMS.lock().unwrap();
            for item in guard.1.iter_mut() {
                if item.leaving.is_none() && now.duration_since(item.born) + Duration::from_millis(OUT_MS) >= item.life {
                    item.leaving = Some(now);
                }
            }
            guard.1.retain(|i| i.leaving.is_none_or(|t| now.duration_since(t) < Duration::from_millis(OUT_MS)));
            guard.1.clone()
        };
        if items.is_empty() {
            return None;
        }
        // Ömür ve çıkış zamanlaması için kartlar görünürken kare istenir (en fazla 7 sn).
        window.request_animation_frame();
        // Electron kartları `body`'ye taşır: `.app-frame .p2-toasts { top: 48px }` uygulanmaz → 20px
        // (başlık çubuğunun üstüne biner).
        let mut stack = div().absolute().top(px(20.)).right(px(20.)).flex().flex_col().gap(px(10.));
        for item in items {
            let id = item.id;
            let kicker: SharedString = match item.tone {
                ToastTone::Ok => "TAMAM".into(),
                ToastTone::Error => "HATA".into(),
                ToastTone::Info => "P2".into(),
            };
            let (title, body) = match item.title {
                Some(t) => (t, Some(SharedString::from(item.text))),
                None => (item.text, None),
            };
            let close = button(("toast-close", id as usize), BtnVariant::Ghost, BtnSize::Sm, true, false, p)
                .child("×")
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    dismiss(id);
                    cx.notify();
                }))
                .into_any_element();
            let card = toast(("toast", id as usize), p2_ui::components::logo_image(44., p).into_any_element(), Some(kicker), title, body, Some(close), item.tone, Some(item.life - Duration::from_millis(OUT_MS)), p);
            let card = div().child(card);
            stack = stack.child(match item.leaving {
                // `p2-slide-right-out`: opaklık 1 → 0, sağa (genişlik + 48px), ease-in
                Some(_) => card
                    .with_animation(("toast-out", id as usize), gpui_kit::Animation::new(Duration::from_millis(OUT_MS)), |el, t| {
                        let k = p2_ui::motion::EASE_IN.ease(t);
                        el.opacity(1. - t).relative().left(px(400. * k))
                    })
                    .into_any_element(),
                None => card.into_any_element(),
            });
        }
        Some(stack.into_any_element())
    }
}
