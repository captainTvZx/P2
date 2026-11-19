//! Kütüphane otomatik güncellemesi (src/library.ts `useLibraryAutoUpdate`): kurulu oyunun
//! kategorisinde yeni sürüm canlıya alınınca (Steam gibi) kendiliğinden indirilir.
//!
//! - Açılışta kurulu her proje için `releases/latest?channel=` denetlenir.
//! - `release:live` gerçek zamanlı olayı gelince aynı karar verilir; `release:published` yalnız
//!   bildirimdir (masaüstü ya da oyun içi kart).
//! - Elle seçilmiş (sabitlenmiş) sürüm, farklı kategori, zaten kurulu sürüm ya da ayar kapalıysa
//!   dokunulmaz. Kuyruk tek tek ilerler: başka kurulum işi sürerken beklenir.
//! - Hız sınırı patcher'ın ortak sınırlayıcısından gelir; oyun açıkken indirme sürer ama montaj
//!   oyun kapanana dek bekler (patcher `waiting-for-exit`, patcher.cjs ile aynı).

use std::collections::VecDeque;
use std::time::Duration;

use gpui_kit::{Context, Window};
use p2_client_core::ReleaseView;

use super::WorkspaceScreen;

struct Pending {
    project: String,
    release: String,
    channel: String,
    version: String,
}

#[derive(Default)]
pub(super) struct AutoUpdate {
    queue: VecDeque<Pending>,
    /// Kuyruktan başlatılan ve henüz bitmeyen kurulumun projesi.
    pumping: Option<String>,
    /// Raydaki kütüphane düğmesinin rozeti: süren indirmelerin toplam yüzdesi ve hepsi duraklatıldı mı.
    pub(super) rail: Option<(u32, bool)>,
}

impl WorkspaceScreen {
    /// Çalışma alanı yüklenince kurulu projelerin canlı sürümünü bir kez denetler.
    pub(super) fn start_auto_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // İstemci güncelleyicisi (updater.cjs): kaynak bağlanılan sunucu; overlay'li oyun açıkken
        // otomatik kurulum ertelenir (main.cjs `setIdleCheck`).
        crate::updates::configure(self.api.base());
        let overlay = self.overlay.clone();
        crate::updates::updater().set_idle_check(std::sync::Arc::new(move || overlay.active() == 0));
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(600)).await;
                match this.update(cx, |this, _| !this.loading) {
                    Ok(true) => break,
                    Ok(false) => continue,
                    Err(_) => return,
                }
            }
            let _ = this.update_in(cx, |this, window, cx| this.auto_update_check(window, cx));
        })
        .detach();
    }

    fn auto_update_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let installed: Vec<(String, String)> = self.patcher.state().projects.iter().map(|(id, p)| (id.clone(), p.channel.clone())).collect();
        crate::trace(&format!("kütüphane: otomatik güncelleme denetimi ({} kurulu)", installed.len()));
        if installed.is_empty() {
            return;
        }
        let (api, org) = (self.api.clone(), self.org.id.clone());
        crate::task::run(
            window,
            cx,
            move || installed.into_iter().map(|(project, channel)| (project.clone(), api.latest_release(&org, &project, &channel))).collect::<Vec<_>>(),
            |this, results, _, cx| {
                for (project, result) in results {
                    match result {
                        Ok(Some(release)) => this.auto_consider(&project, release, cx),
                        Ok(None) => {}
                        Err(e) => crate::trace(&format!("kütüphane: canlı sürüm alınamadı ({project}): {}", e.message)),
                    }
                }
            },
        );
    }

    /// Gerçek zamanlı `release:published` / `release:live`. İşlendiyse true.
    pub(super) fn on_release_event(&mut self, name: &str, payload: &serde_json::Value, cx: &mut Context<Self>) -> bool {
        let live = match name {
            "release:live" => true,
            "release:published" => false,
            // realtime.ts: sunucuya yeni istemci sürümü yayınlandı → hemen denetle.
            "update:published" => {
                crate::trace(&format!("olay: update:published {}", payload["version"].as_str().unwrap_or_default()));
                crate::updates::check();
                return true;
            }
            _ => return false,
        };
        let Ok(release) = serde_json::from_value::<ReleaseView>(payload["release"].clone()) else { return true };
        let project = payload["projectId"].as_str().unwrap_or_default().to_string();
        let project_name = payload["projectName"].as_str().map(str::to_string).or_else(|| self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone())).unwrap_or_default();
        crate::trace(&format!("olay: {name} {project_name} {} ({})", release.version, release.channel));
        let routed = self.overlay_release(&project, &project_name, &release.version, live);
        if live {
            self.auto_consider(&project, release, cx);
        } else if !routed {
            let label = if project_name.is_empty() { p2_i18n::t("aupd.library") } else { project_name.clone() };
            let title = format!("{} · {}", label, release.version);
            crate::desktop_notify(&title, &p2_i18n::t("aupd.newRelease"));
        }
        true
    }

    /// library.ts `consider`: kurulu, sabitlenmemiş, aynı kategoride ve farklı sürümse kuyruğa.
    fn auto_consider(&mut self, project: &str, release: ReleaseView, cx: &mut Context<Self>) {
        let Some(installed) = self.patcher.state().projects.get(project).cloned() else { return };
        if installed.pinned || installed.release_id == release.id || installed.channel != release.channel {
            if installed.pinned && installed.release_id != release.id {
                crate::trace(&format!("kütüphane: otomatik güncelleme atlandı (sabitlenmiş sürüm) {}", release.version));
            }
            return;
        }
        if self.auto.queue.iter().any(|p| p.project == project) || self.auto.pumping.as_deref() == Some(project) {
            return;
        }
        let enabled = self.app.upgrade().is_some_and(|a| a.read(cx).settings.library.auto_download);
        if !enabled {
            crate::trace(&format!("kütüphane: yeni sürüm {} var, otomatik indirme kapalı", release.version));
            return;
        }
        crate::trace(&format!("kütüphane: otomatik güncelleme kuyruğa alındı {} → {}", installed.version, release.version));
        self.auto.queue.push_back(Pending { project: project.into(), release: release.id, channel: release.channel, version: release.version });
        self.auto_pump();
    }

    /// Sırada bekleyen ilk güncellemeyi başlatır (başka iş sürmüyorsa).
    fn auto_pump(&mut self) {
        if self.auto.pumping.is_some() || self.patcher.any_job_active() {
            return;
        }
        let Some(next) = self.auto.queue.pop_front() else { return };
        let name = self.projects.iter().find(|p| p.id == next.project).map(|p| p.name.clone()).unwrap_or_else(|| p2_i18n::t("aupd.project"));
        crate::trace(&format!(
            "kütüphane: otomatik güncelleme başladı {name} {} {} (hız sınırı {}, oyun {})",
            next.channel,
            next.version,
            match self.patcher.limit_bytes_per_sec() {
                0 => "yok".to_string(),
                b => format!("{} MB/s", b / (1024 * 1024)),
            },
            if self.patcher.is_running(&next.project) { "açık — montaj kapanınca" } else { "kapalı" }
        ));
        self.auto.pumping = Some(next.project.clone());
        self.patcher.install(self.org.id.clone(), next.project, name, next.release, next.channel, next.version, Some(false));
    }

    /// Kurulum işi bitti (başarılı/başarısız/iptal): indirme kartı ve kuyruğun devamı.
    pub(super) fn auto_on_job(&mut self, job: &p2_patcher::JobState, cx: &mut Context<Self>) {
        // Ray rozeti: yalnız tam yüzde değişince yeniden çizilir.
        let rail = {
            let jobs: Vec<_> = self.patcher.jobs().into_iter().filter(|j| j.active).collect();
            let (done, total) = jobs.iter().fold((0u64, 0u64), |(d, t), j| if j.phase == "installing" { (d + j.bytes_written, t + j.bytes_to_write) } else { (d + j.bytes_done, t + j.bytes_total) });
            (!jobs.is_empty()).then(|| (if total > 0 { ((done as f64 / total as f64) * 100.).floor().min(100.) as u32 } else { 0 }, jobs.iter().all(|j| j.paused)))
        };
        if rail != self.auto.rail {
            self.auto.rail = rail;
            cx.notify();
        }
        if !job.active {
            let name = self.projects.iter().find(|p| p.id == job.project_id).map(|p| p.name.clone()).unwrap_or_default();
            match job.phase.as_str() {
                "done" => {
                    crate::toasts::ok(p2_i18n::tv("lib.dl.doneToast", &[("name", &name)]));
                    crate::desktop_notify(&p2_i18n::tv("lib.dl.doneToast", &[("name", &name)]), &p2_i18n::t("lib.dl.doneBody"));
                }
                "failed" => {
                    let msg = p2_i18n::tv("lib.dl.failedToast", &[("name", &name)]);
                    crate::toasts::error(format!("{msg} {}", job.error.clone().unwrap_or_default()).trim().to_string());
                    crate::desktop_notify(&msg, &job.error.clone().unwrap_or_default());
                }
                _ => {}
            }
        }
        if job.active {
            if job.phase == "waiting-for-exit" {
                crate::trace("kütüphane: iş waiting-for-exit (oyun kapanınca kurulacak)");
            }
            return;
        }
        if job.phase == "done" {
            let name = self.projects.iter().find(|p| p.id == job.project_id).map(|p| p.name.clone()).unwrap_or_default();
            self.overlay_download_done(&job.project_id, &name);
        }
        if self.auto.pumping.as_deref() == Some(job.project_id.as_str()) {
            crate::trace(&format!("kütüphane: otomatik güncelleme bitti ({})", job.phase));
            self.auto.pumping = None;
        }
        // library.ts: bir sonraki 500 ms sonra.
        if !self.auto.queue.is_empty() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                let _ = this.update(cx, |this, _| this.auto_pump());
            })
            .detach();
        }
    }
}
