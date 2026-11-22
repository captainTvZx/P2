//! Oyun başlatma kartı: Oyna'ya basılınca ekranın ortasında odak almayan kart açılır
//! (oyunun ikonu + kapak + "Hazırlanıyor…"), oyun açılınca aşama değişir. Kapanma:
//! - overlay ile başlatıldıysa oyun ilk kareyi çizince (overlay "hazır"),
//! - overlay kare alamazsa hemen + "… çalışıyor" bildirimi,
//! - düz başlatmada 4 sn sonra + "… başlatıldı" bildirimi,
//! - başlatılamazsa ya da oyun kapanınca hemen, en geç 30 sn sonra kendiliğinden.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{Context, RenderImage, WindowHandle};

use super::WorkspaceScreen;
use crate::splash::{LaunchCardView, LaunchPhase};

pub(super) struct LaunchCard {
    project: String,
    window: WindowHandle<LaunchCardView>,
    /// Gecikmeli kapatma işleri eski karta dokunmasın.
    epoch: u64,
}

static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl WorkspaceScreen {
    fn project_name(&self, project: &str) -> String {
        self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()).unwrap_or_else(|| p2_i18n::t("appx.game"))
    }

    /// Oyna'ya basıldı (`error` yok) ya da başlatma başarısız oldu.
    pub(super) fn launch_card_on_launching(&mut self, project: &str, failed: bool, cx: &mut Context<Self>) {
        if failed {
            self.close_launch_card(Some(project), "başlatılamadı", cx);
            return;
        }
        self.open_launch_card(project, LaunchPhase::Preparing, cx);
    }

    pub(super) fn launch_card_on_running(&mut self, project: &str, running: bool, cx: &mut Context<Self>) {
        if !running {
            self.close_launch_card(Some(project), "oyun kapandı", cx);
            return;
        }
        let overlay = self.overlay.has_project(project);
        let phase = if overlay { LaunchPhase::Overlay } else { LaunchPhase::Starting };
        // Kart Oyna'da açıldıysa yalnız aşaması değişir; değilse (bağlantıdan başlatma vb.) açılır.
        let epoch = match self.launch_card.as_ref().filter(|c| c.project == project) {
            Some(card) => {
                let _ = card.window.update(cx, |v, _, cx| v.set_phase(phase, cx));
                card.epoch
            }
            None => match self.open_launch_card(project, phase, cx) {
                Some(e) => e,
                None => return,
            },
        };
        let name = self.project_name(project);
        crate::trace(&format!("başlatma kartı: oyun açıldı «{name}» ({})", if overlay { "overlay hazır olunca kapanır" } else { "düz başlatma" }));
        if overlay {
            return;
        }
        cx.spawn(async move |this, cx| {
            // Düz başlatma: 4 sn sonra kapanır ve Windows bildirimi.
            cx.background_executor().timer(Duration::from_secs(4)).await;
            let _ = this.update(cx, |this, cx| {
                if this.launch_card.as_ref().is_some_and(|c| c.epoch == epoch) {
                    this.close_launch_card(None, "düz başlatma (4 sn)", cx);
                    crate::desktop_notify(&p2_i18n::tv("appx.launch.started", &[("name", name.as_str())]), &p2_i18n::t("appx.launch.noOverlay"));
                }
            });
        })
        .detach();
    }

    /// Kartı açar (varsa eskisini kapatır), oyunun ikonunu ve kapağını arka planda yükler.
    fn open_launch_card(&mut self, project: &str, phase: LaunchPhase, cx: &mut Context<Self>) -> Option<u64> {
        self.close_launch_card(None, "yenisi açılıyor", cx);
        let name = self.project_name(project);
        let window = crate::splash::open_launch_card(cx, &name, phase)?;
        let epoch = EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        crate::trace(&format!("başlatma kartı: açıldı «{name}» ({phase:?})"));
        self.launch_card = Some(LaunchCard { project: project.to_string(), window, epoch });
        // Görseller: exe ikonu (yoksa proje ikonu) + kapak.
        let info = self.projects.iter().find(|p| p.id == project);
        let (icon_url, cover_url) = (info.and_then(|p| p.icon_url.clone()), info.and_then(|p| p.cover_url.clone()));
        let exe = {
            let state = self.patcher.state();
            state.projects.get(project).map(|i| state.project_dir(&i.folder).join(i.executable.replace('/', std::path::MAIN_SEPARATOR_STR)))
        };
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let (icon, cover) = cx
                .background_executor()
                .spawn(async move {
                    let fetch = |url: Option<String>| -> Option<Arc<RenderImage>> {
                        let url = url.filter(|u| !u.trim().is_empty())?;
                        let path = url.find("/uploads/").map(|i| url[i..].to_string()).unwrap_or(url);
                        let bytes = api.get_bytes(&path).ok()?;
                        let mut rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
                        for px in rgba.pixels_mut() {
                            px.0.swap(0, 2);
                        }
                        Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
                    };
                    // Önce P2'de proje için belirlenen ikon; yoksa exe ikonu.
                    let icon = fetch(icon_url).map(|i| (i, true)).or_else(|| exe.as_deref().and_then(|e| crate::exe_icon::load(e, 256)).map(|i| (i, false)));
                    (icon, fetch(cover_url))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(card) = this.launch_card.as_ref().filter(|c| c.epoch == epoch) {
                    let _ = card.window.update(cx, |v, _, cx| v.set_art(icon, cover, cx));
                }
            });
            // Emniyet: overlay hiç "hazır" demezse kart kendiliğinden kapanır.
            cx.background_executor().timer(Duration::from_secs(30)).await;
            let _ = this.update(cx, |this, cx| {
                if this.launch_card.as_ref().is_some_and(|c| c.epoch == epoch) {
                    this.close_launch_card(None, "30 sn emniyet", cx);
                }
            });
        })
        .detach();
        Some(epoch)
    }

    /// `project` verilirse yalnız o projenin kartı kapanır.
    pub(super) fn close_launch_card(&mut self, project: Option<&str>, why: &str, cx: &mut Context<Self>) {
        if project.is_some_and(|p| self.launch_card.as_ref().is_some_and(|c| c.project != p)) {
            return;
        }
        if let Some(card) = self.launch_card.take() {
            crate::splash::close_window(cx, card.window.into());
            crate::trace(&format!("başlatma kartı: kapandı ({why})"));
        }
    }

    /// Overlay kare alamadı (D3D12/Vulkan/OpenGL olabilir): kart kapanır, Windows bildirimi.
    pub(super) fn launch_card_no_overlay(&mut self, project: &str, cx: &mut Context<Self>) {
        let name = self.project_name(project);
        self.close_launch_card(Some(project), "overlay bağlanamadı", cx);
        crate::desktop_notify(&p2_i18n::tv("appx.launch.running", &[("name", name.as_str())]), &p2_i18n::t("appx.launch.overlayFailed"));
    }
}
