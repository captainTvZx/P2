//! Toasts.tsx — uygulama içi kısa bildirimler (sağ üst, üstten 20px; en çok 3 kart).
//! Uygulamadaki BÜTÜN bildirimler buradan geçer (ekranların kendi kırmızı kutuları / alt şeritleri yok):
//! [`ok`] (başarı), [`info`] (bilgi), [`warn`] (uyarı), [`error`] (hata) ya da başlıklı kart için [`push`].
//! Her yerden çağrılır (iş parçacığı güvenli); kök görünüm [`wake_channel`] ile uyandırılır. Ömür 7 sn,
//! çıkış `p2-slide-right-out` 360ms ease-in. Kart görünümü `p2_ui::controls::toast` (ui.css `.p2-toast`).
//! Form doğrulama hataları bildirim değildir; alanın yanında kalır.
//! İz: `bildirim[ok|bilgi|uyarı|hata]: <metin>` (testler).
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
/// Yığının üst boşluğu: başlık çubuğu 38 + sayfa başlığı 54 + 12 aralık.
const TOP: f32 = 38. + 54. + 12.;

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
/// Kök görünümü uyandıran kanal: bildirim başka bir görünümden / iş parçacığından gelse de hemen çizilir.
static WAKE: Mutex<Option<futures::channel::mpsc::UnboundedSender<()>>> = Mutex::new(None);

/// Kök görünüm (`ClientApp::new`) alıcıyı dinler ve her bildirimde yeniden çizer.
pub fn wake_channel() -> futures::channel::mpsc::UnboundedReceiver<()> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    *WAKE.lock().unwrap() = Some(tx);
    rx
}

/// Başarı ("TAMAM"): işlem tamamlandı.
pub fn ok(text: impl AsRef<str>) {
    push(ToastTone::Ok, None, text.as_ref());
}

/// Bilgi ("P2"): yansız haber.
pub fn info(text: impl AsRef<str>) {
    push(ToastTone::Info, None, text.as_ref());
}

/// Uyarı ("UYARI"): işlem yapılmadı ama hata da değil (yetki yok, önce şunu yap…).
pub fn warn(text: impl AsRef<str>) {
    push(ToastTone::Warn, None, text.as_ref());
}

/// Hata ("HATA"): kırmızı, aynı sağ üst kart.
pub fn error(text: impl AsRef<str>) {
    push(ToastTone::Error, None, text.as_ref());
}

/// Toasts.tsx `pushToast({text, tone, title})`.
pub fn push(tone: ToastTone, title: Option<&str>, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    crate::trace(&format!(
        "bildirim[{}]: {}{}",
        match tone {
            ToastTone::Ok => "ok",
            ToastTone::Info => "bilgi",
            ToastTone::Warn => "uyarı",
            ToastTone::Error => "hata",
        },
        title.map(|t| format!("{t} — ")).unwrap_or_default(),
        text
    ));
    let mut guard = ITEMS.lock().unwrap();
    // Aynı metin zaten ekrandaysa yenisi eklenmez (art arda tıklamalar yığın yapmasın); süresi baştan başlar.
    guard.0 += 1;
    let next = guard.0;
    if let Some(same) = guard.1.iter_mut().find(|i| i.leaving.is_none() && i.tone == tone && i.text == text && i.title.as_deref() == title) {
        same.born = Instant::now();
        same.id = next;
        drop(guard);
        wake();
        return;
    }
    let id = next;
    let list = &mut guard.1;
    // `[...list.slice(-2), yeni]`
    while list.len() > 2 {
        list.remove(0);
    }
    list.push(Item { id, tone, title: title.map(str::to_string), text: text.to_string(), life: Duration::from_millis(7000), born: Instant::now(), leaving: None });
    drop(guard);
    wake();
}

fn wake() {
    if let Some(tx) = WAKE.lock().unwrap().as_ref() {
        let _ = tx.unbounded_send(());
    }
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
            "warn" => push(ToastTone::Warn, Some("Yetkin yok"), "Bu işlem için proje yöneticisine danış"),
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
        // Başlık çubuğunun (38px) ve sayfa başlıklarının (54px; sağdaki eylem düğmeleri, ör. "Arkadaş
        // ekle") altında: eskiden 20px'te bu düğmelerin üstüne biniyordu.
        let mut stack = div().absolute().top(px(TOP)).right(px(20.)).flex().flex_col().gap(px(10.));
        for item in items {
            let id = item.id;
            let kicker: SharedString = match item.tone {
                ToastTone::Ok => p2_i18n::t("toast.ok").into(),
                ToastTone::Error => p2_i18n::t("toast.error").into(),
                ToastTone::Warn => p2_i18n::t("toast.warning").into(),
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
            // Kimlikli sarmalayıcı: kartın giriş canlandırması ("p2-toast") her kart için ayrı durum tutsun.
            let card = div().id(("toast-wrap", id as usize)).child(card);
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
