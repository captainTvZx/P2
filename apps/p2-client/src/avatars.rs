//! Profil fotoğrafları: kullanıcı kimliği → `/uploads/...` yolu (üye listelerinden öğrenilir),
//! yol → çözülmüş görsel. GPUI'de uzak resim yükleyici olmadığından kimlikli indirilip
//! 128 px kare kırpılır; inince pencereler yeniden çizilir (`DIRTY`). `p2_ui::avatar` bu
//! önbelleğe kaynak olarak bağlanır, böylece bütün avatarlar kendiliğinden fotoğrafa döner.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::RenderImage;
use p2_client_core::Api;

enum Slot {
    Loading,
    Ready(Arc<RenderImage>),
    Failed,
}

struct Cache {
    urls: HashMap<String, String>,
    images: HashMap<String, Slot>,
    api: Option<Arc<Api>>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache { urls: HashMap::new(), images: HashMap::new(), api: None }))
}

/// Yeni görsel indi: pencereler yeniden çizilmeli.
pub static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn install(api: Arc<Api>) {
    cache().lock().unwrap().api = Some(api);
    p2_ui::components::set_avatar_source(source);
}

/// Kullanıcının fotoğraf yolunu kaydet (`None` ya da boş: fotoğraf yok).
pub fn remember(user: &str, url: Option<&str>) {
    let mut c = cache().lock().unwrap();
    match url.filter(|u| !u.trim().is_empty()) {
        Some(u) => {
            let path = u.find("/uploads/").map(|i| u[i..].to_string()).unwrap_or_else(|| u.to_string());
            if c.urls.get(user) != Some(&path) {
                c.urls.insert(user.to_string(), path);
                DIRTY.store(true, Ordering::Relaxed);
            }
        }
        None => {
            if c.urls.remove(user).is_some() {
                DIRTY.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn decode(bytes: &[u8]) -> Option<Arc<RenderImage>> {
    let img = image::load_from_memory(bytes).ok()?;
    let side = img.width().min(img.height());
    let square = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side).resize_exact(128, 128, image::imageops::FilterType::Triangle);
    let mut bgra = square.to_rgba8();
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])))
}

fn source(user: &str) -> Option<Arc<RenderImage>> {
    let mut c = cache().lock().unwrap();
    let url = c.urls.get(user)?.clone();
    match c.images.get(&url) {
        Some(Slot::Ready(img)) => return Some(img.clone()),
        Some(_) => return None,
        None => {}
    }
    let api = c.api.clone()?;
    c.images.insert(url.clone(), Slot::Loading);
    drop(c);
    std::thread::spawn(move || {
        let slot = api.get_bytes(&url).ok().and_then(|b| decode(&b)).map(Slot::Ready).unwrap_or(Slot::Failed);
        let ready = matches!(slot, Slot::Ready(_));
        cache().lock().unwrap().images.insert(url.clone(), slot);
        crate::trace(&format!("avatar: {url} {}", if ready { "hazır" } else { "yüklenemedi" }));
        DIRTY.store(true, Ordering::Relaxed);
    });
    None
}

/// Hazır fotoğraf (yoksa indirmeyi başlatır) — kare/yuvarlatılmış avatar çizen ekranlar için (B: kenar çubuğu).
pub fn image(user: &str) -> Option<Arc<RenderImage>> {
    source(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_paths() {
        remember("u1", Some("http://10.0.0.1:4000/uploads/a.png"));
        assert_eq!(cache().lock().unwrap().urls.get("u1").map(String::as_str), Some("/uploads/a.png"));
        remember("u1", None);
        assert!(!cache().lock().unwrap().urls.contains_key("u1"));
    }
}
