//! Keşfet afişleri: `/uploads/...` yolu → çözülmüş görsel. `avatars.rs` kare kırptığı için afişlere
//! uymaz; burada görsel en-boy oranı korunarak en çok 960px genişliğe indirilir ve yol başına bir kez
//! çözülür. İnince pencereler yeniden çizilir (`avatars::DIRTY`).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::RenderImage;
use p2_client_core::Api;

enum Slot {
    Loading,
    Ready(Arc<RenderImage>),
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

fn decode(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = if img.width() > MAX_W { img.resize(MAX_W, u32::MAX, image::imageops::FilterType::Triangle) } else { img };
    let mut bgra = img.to_rgba8();
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
}

/// Hazır afiş; yoksa indirmeyi başlatır ve `None` döner (inince yeniden çizilir).
pub fn cover(url: Option<&str>) -> Option<Arc<RenderImage>> {
    let url = url.filter(|u| !u.trim().is_empty())?;
    let path = url.find("/uploads/").map(|i| url[i..].to_string()).unwrap_or_else(|| url.to_string());
    let mut c = cache().lock().unwrap();
    match c.images.get(&path) {
        Some(Slot::Ready(img)) => return Some(img.clone()),
        Some(_) => return None,
        None => {}
    }
    let api = c.api.clone()?;
    c.images.insert(path.clone(), Slot::Loading);
    drop(c);
    std::thread::spawn(move || {
        let slot = api.get_bytes(&path).ok().and_then(|b| decode(&b)).map(Slot::Ready).unwrap_or(Slot::Failed);
        cache().lock().unwrap().images.insert(path, slot);
        crate::avatars::DIRTY.store(true, Ordering::Relaxed);
    });
    None
}
