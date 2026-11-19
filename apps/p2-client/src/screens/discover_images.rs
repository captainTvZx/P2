//! Keşfet afişleri: `/uploads/...` yolu → çözülmüş görsel. `avatars.rs` kare kırptığı için afişlere
//! uymaz; burada görsel en-boy oranı korunarak en çok 960px genişliğe indirilir ve yol başına bir kez
//! çözülür. İnince pencereler yeniden çizilir (`avatars::DIRTY`). Hareketli GIF afişler oynar
//! (kareler en çok 640px genişlikte; bkz. `avatars::Frames`).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::RenderImage;
use p2_client_core::Api;

use crate::avatars::Frames;

enum Slot {
    Loading,
    Ready(Frames),
    Failed,
}

struct Cache {
    images: HashMap<String, Slot>,
    api: Option<Arc<Api>>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache { images: HashMap::new(), api: None }))
}

pub fn install(api: Arc<Api>) {
    cache().lock().unwrap().api = Some(api);
}

const MAX_W: u32 = 960;

/// Hareketli afişte kare genişliği ve toplam bellek sınırı (aşılırsa kareler seyreltilir).
const MAX_ANIM_W: u32 = 640;
const MAX_ANIM_BYTES: usize = 64 * 1024 * 1024;

fn decode(bytes: &[u8]) -> Option<Frames> {
    Frames::decode(bytes, 150, MAX_ANIM_BYTES, |img, animated| {
        let max = if animated { MAX_ANIM_W } else { MAX_W };
        let img = if img.width() > max { img.resize(max, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
        img.to_rgba8()
    })
}

/// Yalnız hareketli (GIF) afiş: durağan kapağı kendi tam boyutunda çizen yerler için (Kütüphane).
pub fn animated_cover(url: Option<&str>) -> Option<Arc<RenderImage>> {
    let image = cover(url)?;
    let path = url.map(|u| u.find("/uploads/").map(|i| u[i..].to_string()).unwrap_or_else(|| u.to_string()))?;
    matches!(cache().lock().unwrap().images.get(&path), Some(Slot::Ready(f)) if f.animated()).then_some(image)
}

/// Hazır afiş; yoksa indirmeyi başlatır ve `None` döner (inince yeniden çizilir).
pub fn cover(url: Option<&str>) -> Option<Arc<RenderImage>> {
    let url = url.filter(|u| !u.trim().is_empty())?;
    let path = url.find("/uploads/").map(|i| url[i..].to_string()).unwrap_or_else(|| url.to_string());
    let mut c = cache().lock().unwrap();
    match c.images.get(&path) {
        Some(Slot::Ready(frames)) => return Some(frames.current()),
        Some(_) => return None,
        None => {}
    }
    let api = c.api.clone().or_else(crate::avatars::api)?;
    c.images.insert(path.clone(), Slot::Loading);
    drop(c);
    std::thread::spawn(move || {
        let slot = api.get_bytes(&path).ok().and_then(|b| decode(&b)).map(Slot::Ready).unwrap_or(Slot::Failed);
        cache().lock().unwrap().images.insert(path, slot);
        crate::avatars::DIRTY.store(true, Ordering::Relaxed);
    });
    None
}
