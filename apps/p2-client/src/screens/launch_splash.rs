//! Oyun başlatma kartı (patcher.cjs `showLaunchSplash` / `closeSplash` akışı): oyun başlayınca
//! ekranın ortasında odak almayan küçük "P2 · Başlatılıyor" kartı. Kapanma:
//! - overlay ile başlatıldıysa oyun ilk kareyi çizince (overlay "hazır"),
//! - overlay kare alamazsa hemen + "… çalışıyor" bildirimi,
//! - düz başlatmada 4 sn sonra + "… başlatıldı" bildirimi,
//! - oyun kapanınca hemen, en geç 20 sn sonra kendiliğinden.

use std::time::Duration;

use gpui_kit::{AnyWindowHandle, Context};

use super::WorkspaceScreen;

pub(super) struct LaunchCard {
    project: String,
    window: AnyWindowHandle,
    /// Gecikmeli kapatma işleri eski karta dokunmasın.
    epoch: u64,
}

static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl WorkspaceScreen {
    pub(super) fn launch_card_on_running(&mut self, project: &str, running: bool, cx: &mut Context<Self>) {
        if !running {
            self.close_launch_card(Some(project), "oyun kapandı", cx);
            return;
        }
        self.close_launch_card(Some(project), "yenisi açılıyor", cx);
        let name = self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()).unwrap_or_else(|| p2_i18n::t("appx.game"));
        let Some(window) = crate::splash::open_launch_card(cx, &name) else { return };
        let epoch = EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let overlay = self.overlay.has_project(project);
        crate::trace(&format!("başlatma kartı: açıldı «{name}» ({})", if overlay { "overlay hazır olunca kapanır" } else { "düz başlatma" }));
        self.launch_card = Some(LaunchCard { project: project.to_string(), window, epoch });
        let plain = !overlay;
        cx.spawn(async move |this, cx| {
            // Düz başlatma: 4 sn sonra kapanır ve Windows bildirimi; her durumda 20 sn emniyet.
            if plain {
                cx.background_executor().timer(Duration::from_secs(4)).await;
                let _ = this.update(cx, |this, cx| {
                    if this.launch_card.as_ref().is_some_and(|c| c.epoch == epoch) {
                        this.close_launch_card(None, "düz başlatma (4 sn)", cx);
                        crate::desktop_notify(&p2_i18n::tv("appx.launch.started", &[("name", name.as_str())]), &p2_i18n::t("appx.launch.noOverlay"));
                    }
                });
            }
            cx.background_executor().timer(Duration::from_secs(if plain { 16 } else { 20 })).await;
            let _ = this.update(cx, |this, cx| {
                if this.launch_card.as_ref().is_some_and(|c| c.epoch == epoch) {
                    this.close_launch_card(None, "20 sn emniyet", cx);
                }
            });
        })
        .detach();
    }

    /// `project` verilirse yalnız o projenin kartı kapanır.
    pub(super) fn close_launch_card(&mut self, project: Option<&str>, why: &str, cx: &mut Context<Self>) {
        if project.is_some_and(|p| self.launch_card.as_ref().is_some_and(|c| c.project != p)) {
            return;
        }
        if let Some(card) = self.launch_card.take() {
            crate::splash::close_window(cx, card.window);
            crate::trace(&format!("başlatma kartı: kapandı ({why})"));
        }
    }

    /// Overlay kare alamadı (D3D12/Vulkan/OpenGL olabilir): kart kapanır, Windows bildirimi.
    pub(super) fn launch_card_no_overlay(&mut self, project: &str, cx: &mut Context<Self>) {
        let name = self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()).unwrap_or_else(|| p2_i18n::t("appx.game"));
        self.close_launch_card(Some(project), "overlay bağlanamadı", cx);
        crate::desktop_notify(&p2_i18n::tv("appx.launch.running", &[("name", name.as_str())]), &p2_i18n::t("appx.launch.overlayFailed"));
    }
}
