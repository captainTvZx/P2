//! Oyun içi bildirim kuyruğu — Electron `src/overlay/toastStore.ts` birebir: en çok 3 kart ekranda,
//! fazlası sırada; aynı `tag`'li yeni kart eskisinin yerini alır (süre sıfırlanır); `sticky` kartın süresi
//! yok; kapanırken kart önce `leaving` olur (360 ms çıkış animasyonu), sonra listeden düşer.

use std::time::{Duration, Instant};

use p2_overlay_ipc::proto::Toast;

pub const MAX_VISIBLE: usize = 3;
pub const OUT: Duration = Duration::from_millis(360);
const DEFAULT_MS: u64 = 6500;

pub fn life(t: &Toast) -> Duration {
    Duration::from_millis(t.duration_ms.filter(|d| *d > 0).unwrap_or(DEFAULT_MS).clamp(2000, 20000))
}

pub struct Item {
    pub toast: Toast,
    pub seq: u64,
    pub shown_at: Instant,
    /// Kapanış zamanı (sticky: yok). Geldiğinde `leaving` olur.
    pub close_at: Option<Instant>,
    pub leaving: Option<Instant>,
}

#[derive(Default)]
pub struct Toasts {
    pub visible: Vec<Item>,
    queue: Vec<Item>,
    seq: u64,
}

impl Toasts {
    fn arm(item: &mut Item) {
        item.shown_at = Instant::now();
        item.close_at = (!item.toast.sticky).then(|| Instant::now() + life(&item.toast) - OUT);
    }
    fn promote(&mut self) {
        while self.visible.iter().filter(|i| i.leaving.is_none()).count() < MAX_VISIBLE && !self.queue.is_empty() {
            let mut next = self.queue.remove(0);
            Self::arm(&mut next);
            self.visible.push(next);
        }
    }

    pub fn push(&mut self, mut toast: Toast) {
        self.seq += 1;
        if toast.id.is_empty() {
            toast.id = format!("t{}", self.seq);
        }
        let mut item = Item { toast, seq: self.seq, shown_at: Instant::now(), close_at: None, leaving: None };
        if let Some(tag) = item.toast.tag.clone() {
            self.visible.retain(|e| !(e.toast.tag.as_deref() == Some(tag.as_str()) && e.leaving.is_some()));
            if let Some(i) = self.visible.iter().position(|e| e.toast.tag.as_deref() == Some(tag.as_str())) {
                Self::arm(&mut item);
                self.visible[i] = item;
                return;
            }
            self.queue.retain(|e| e.toast.tag.as_deref() != Some(tag.as_str()));
        }
        if let Some(i) = self.visible.iter().position(|e| e.toast.id == item.toast.id) {
            Self::arm(&mut item);
            self.visible[i] = item;
            return;
        }
        self.queue.push(item);
        self.promote();
    }

    pub fn dismiss(&mut self, id: &str) {
        match self.visible.iter_mut().find(|e| e.toast.id == id) {
            Some(item) => {
                if item.leaving.is_none() {
                    item.leaving = Some(Instant::now());
                    item.close_at = None;
                }
            }
            None => self.queue.retain(|e| e.toast.id != id),
        }
    }

    pub fn dismiss_tag(&mut self, tag: &str) {
        self.queue.retain(|e| e.toast.tag.as_deref() != Some(tag));
        let ids: Vec<String> = self.visible.iter().filter(|e| e.toast.tag.as_deref() == Some(tag)).map(|e| e.toast.id.clone()).collect();
        for id in ids {
            self.dismiss(&id);
        }
    }

    /// Zamanlayıcılar: süresi dolan kart `leaving`, çıkışı biten kart düşer. Değişiklik varsa true.
    pub fn tick(&mut self, reduce_motion: bool) -> bool {
        let now = Instant::now();
        let mut changed = false;
        for item in self.visible.iter_mut() {
            if item.leaving.is_none() && item.close_at.is_some_and(|t| now >= t) {
                item.leaving = Some(now);
                changed = true;
            }
        }
        let out = if reduce_motion { Duration::from_millis(1) } else { OUT };
        let before = self.visible.len();
        self.visible.retain(|i| i.leaving.is_none_or(|t| now.duration_since(t) < out));
        if before != self.visible.len() {
            changed = true;
            self.promote();
        }
        changed
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }
    pub fn len(&self) -> usize {
        self.visible.len()
    }
    pub fn animating(&self) -> bool {
        let now = Instant::now();
        self.visible.iter().any(|i| i.leaving.is_some() || now.duration_since(i.shown_at) < Duration::from_millis(900))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, tag: Option<&str>) -> Toast {
        Toast { id: id.into(), title: id.into(), tag: tag.map(String::from), ..Default::default() }
    }

    #[test]
    fn kuyruk_ve_etiket() {
        let mut s = Toasts::default();
        for i in 0..5 {
            s.push(t(&format!("a{i}"), None));
        }
        assert_eq!(s.len(), 3);
        s.push(t("c1", Some("call")));
        s.push(t("c2", Some("call")));
        s.dismiss("a0");
        s.dismiss("a1");
        assert_eq!(s.visible.iter().filter(|i| i.leaving.is_none()).count(), 1);
        std::thread::sleep(OUT + Duration::from_millis(20));
        s.tick(false);
        // a2 + kuyruktan a3, a4 → 3; c2 (etiket aynı c1'in yerine) sırada
        assert_eq!(s.len(), 3);
        s.dismiss_tag("call");
        assert!(s.queue.is_empty());
    }
}
