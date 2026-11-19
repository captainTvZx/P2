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

/// Önbellek anahtarı ve indirme yolu: sunucunun kendi yüklemesi (`/uploads/...`; mutlak adreste de
/// sunucu adından hemen sonra gelir) göreli yola indirgenir; başka sitedeki görsel tam adresiyle kalır.
fn cache_key(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        let path = rest.find('/').map(|i| &rest[i..]).unwrap_or("");
        return if path.starts_with("/uploads/") { path.to_string() } else { url.to_string() };
    }
    url.to_string()
}

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
    let path = url.map(cache_key)?;
    matches!(cache().lock().unwrap().images.get(&path), Some(Slot::Ready(f)) if f.animated()).then_some(image)
}

/// Hazır afiş; yoksa indirmeyi başlatır ve `None` döner (inince yeniden çizilir).
pub fn cover(url: Option<&str>) -> Option<Arc<RenderImage>> {
    let url = url.filter(|u| !u.trim().is_empty())?;
    let path = cache_key(url);
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
        // Başka sitedeki görsel (sponsorlu kart): sunucu adresine eklenmeden doğrudan indirilir.
        let bytes = if path.starts_with("http://") || path.starts_with("https://") { fetch_external(&path) } else { api.get_bytes(&path).ok() };
        let slot = bytes.and_then(|b| decode(&b)).map(Slot::Ready).unwrap_or(Slot::Failed);
        cache().lock().unwrap().images.insert(path, slot);
        crate::avatars::DIRTY.store(true, Ordering::Relaxed);
    });
    None
}

/// Dış adresteki görsel (en çok 8 MB, 15 sn).
fn fetch_external(url: &str) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(15)).build().ok()?;
    let resp = client.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mut data = Vec::new();
    resp.take(8 * 1024 * 1024).read_to_end(&mut data).ok()?;
    Some(data)
}
