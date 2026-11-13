//! Oyun içi overlay ile çalışma alanı arasındaki köprü (Electron `main.cjs`'deki overlay
//! bağlantılarının karşılığı): kütüphaneden başlatılan oyunlara enjeksiyon, panelden gelen
//! arama eylemleri, kısayollar, gelen mesaj/arama bildirimleri, arama durumu, belirteç.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt as _;
use gpui_kit::{Context, Window};
use p2_client_core::{CallState, Message, Scope};
use p2_overlay_client::{CallInfo, CallPerson, Context as OverlayContext, Event, HotkeySpec, OverlayManager, Toast};

use super::WorkspaceScreen;

/// Varsayılan kısayollar (Electron ile aynı): 1 panel Shift+2, 2 ekran görüntüsü F12, 3 sustur Shift+M.
pub(super) fn default_hotkeys() -> Vec<HotkeySpec> {
    vec![HotkeySpec::PANEL_DEFAULT, HotkeySpec { id: 2, vk: 0x7b, mods: 0 }, HotkeySpec { id: 3, vk: 0x4d, mods: 0x1 }]
}

impl WorkspaceScreen {
    /// Overlay yöneticisini kurar, kütüphaneye başlatıcı olarak bağlar, olay döngüsünü başlatır.
    pub(super) fn create_overlay(
        api: &Arc<p2_client_core::Api>,
        auth: &p2_client_core::Auth,
        org: &str,
        light: bool,
        patcher: &Arc<p2_patcher::Patcher>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Arc<OverlayManager> {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Event>();
        let overlay = OverlayManager::new(Arc::new(move |e| {
            let _ = tx.unbounded_send(e);
        }));
        overlay.set_context(OverlayContext {
            api: api.base().to_string(),
            client_version: crate::CLIENT_VERSION.to_string(),
            access_token: api.access_token(),
            org: org.to_string(),
            user_id: auth.user.id.clone(),
            user_name: auth.user.display_name.clone(),
            light,
            shot_dir: std::env::var("P2_SHOT_DIR").unwrap_or_default(),
            // Pencere düzeni (Electron localStorage `p2:overlay:layout:<proje>` ile aynı anahtar/biçim).
            layout_path: p2_client_core::store::profile_dir().join("overlay-layout.json").display().to_string(),
            ..Default::default()
        });
        overlay.set_hotkeys(default_hotkeys());
        crate::trace(&format!("overlay: {}", if overlay.available() { "hazır (enjektör + DLL + host bulundu)" } else { "yok (düz başlatma)" }));
        let launcher = overlay.clone();
        patcher.set_launcher(Arc::new(move |spec| launcher.launch(spec)));
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this.update_in(cx, |this, window, cx| this.on_overlay_event(event, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // Erişim belirteci kısa ömürlü: istemci yeniledikçe host'lara iletilir.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(30)).await;
                if this.update(cx, |this, _| this.overlay.set_token(&this.api.access_token())).is_err() {
                    break;
                }
            }
        })
        .detach();
        overlay
    }

    fn on_overlay_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Log { pid, text } => crate::trace(&format!("overlay[{pid}]: {text}")),
            Event::Ready { pid, project_id } => {
                crate::trace(&format!("overlay: oyun hazır (pid {pid})"));
                self.close_launch_card(Some(&project_id), "overlay hazır", cx);
                self.sync_overlay_call(cx);
                self.autopilot_overlay_look(cx);
            }
            Event::NoOverlay { project_id } => {
                crate::trace("overlay: oyun kare çizmedi (desteklenmeyen grafik API'si olabilir)");
                self.launch_card_no_overlay(&project_id, cx);
            }
            Event::Visible { on, .. } => crate::trace(&format!("overlay: panel {}", if on { "açık" } else { "kapalı" })),
            Event::Hotkey { id: 4, down: true, .. } => self.calls.update(cx, |c, cx| c.push_to_talk(true, cx)),
            Event::Hotkey { id, down: true, project_id, .. } => {
                crate::trace(&format!("overlay: kısayol {id}"));
                match id {
                    2 => self.overlay.screenshot(Some(&project_id)),
                    3 => self.calls.update(cx, |c, cx| c.toggle_mute(cx)),
                    5 => self.calls.update(cx, |c, cx| c.toggle_deafen(cx)),
                    _ => {}
                }
            }
            Event::Hotkey { id: 4, down, .. } => self.calls.update(cx, |c, cx| c.push_to_talk(down, cx)),
            Event::Hotkey { .. } => {}
            Event::Action { name, .. } => {
                crate::trace(&format!("overlay: eylem {name}"));
                match name.as_str() {
                    "call-mute" => self.calls.update(cx, |c, cx| c.toggle_mute(cx)),
                    "call-deafen" => self.calls.update(cx, |c, cx| c.toggle_deafen(cx)),
                    "call-leave" => self.calls.update(cx, |c, cx| c.leave(window, cx)),
                    // Bildirim kartı düğmeleri (overlayNotify.ts `planToastAction`).
                    n if n.starts_with("toast:accept:") => self.calls.update(cx, |c, cx| c.accept(window, cx)),
                    n if n.starts_with("toast:decline:") => self.calls.update(cx, |c, cx| c.decline(window, cx)),
                    // Overlay'deki Performans/Ayarlar anahtarları: ayarlara yazılır, bütün oyunlara iletilir.
                    n if n.starts_with("look:") => {
                        if let (Ok(look), Some(app)) = (serde_json::from_str::<p2_overlay_client::Look>(&n[5..]), self.app.upgrade()) {
                            app.update(cx, |app, _| {
                                let o = &mut app.settings.overlay;
                                (o.hud, o.hud_position, o.hud_details, o.toast_corner, o.opacity, o.scale, o.reduce_motion) =
                                    (look.hud, look.hud_position, look.hud_details, look.toast_corner, look.opacity, look.scale, look.reduce_motion);
                                p2_client_core::store::save_settings(&app.settings);
                            });
                            self.apply_prefs(cx);
                        }
                    }
                    _ => {}
                }
            }
            Event::Shot { path, width, height, .. } => crate::trace(&format!("overlay: ekran görüntüsü {path} ({width}x{height})")),
            Event::TokenExpired => {
                // İstemcinin kendi isteği 401'de belirteci yeniler; sonra host'lara iletilir.
                let api = self.api.clone();
                crate::task::run(window, cx, move || api.me().map(|_| api.access_token()), |this, result, _, _| {
                    if let Ok(token) = result {
                        this.overlay.set_token(&token);
                    }
                });
            }
            Event::Ended { .. } => crate::trace("overlay: oyun kapandı"),
        }
    }

    /// Arama durumu → overlay paneli (Arama aracı + HUD).
    pub(super) fn sync_overlay_call(&self, cx: &Context<Self>) {
        // Aramadayken otomatik güncelleme kurulmaz (CallOverlay.tsx `updates.busy`).
        crate::updates::set_busy(self.calls.read(cx).call.is_some());
        if self.overlay.active() == 0 {
            return;
        }
        let calls = self.calls.read(cx);
        let info = match &calls.call {
            Some(call) => CallInfo {
                active: true,
                title: self.call_title(call),
                muted: calls.muted,
                deafened: calls.deafened,
                people: call
                    .participants
                    .iter()
                    .map(|p| CallPerson { name: if p.display_name.is_empty() { "?".into() } else { p.display_name.clone() }, speaking: calls.speaking.contains(&p.user_id), muted: p.muted, user_id: p.user_id.clone(), deafened: false })
                    .collect(),
            },
            None => CallInfo::default(),
        };
        self.overlay.set_call(info);
    }

    /// Doğrudan/grup mesajı oyundayken köşe bildirimi olur.
    pub(super) fn overlay_message(&self, scope: &Scope, message: &Message) {
        if self.overlay.active() == 0 || matches!(scope, Scope::Channel(_)) || !notify_prefs().messages || message.author.id == self.auth.user.id {
            return;
        }
        let name = if message.author.display_name.is_empty() { message.author.username.clone() } else { message.author.display_name.clone() };
        let text: String = message.content.chars().take(140).collect();
        self.overlay.toast(None, Toast { kind: "message".into(), title: name.clone(), text, avatar: Some(p2_overlay_client::ToastAvatar { user_id: message.author.id.clone(), name }), actions: vec![p2_overlay_client::ToastAction { id: "open-chat".into(), label: "Aç".into(), primary: true }], ..Default::default() });
    }

    /// overlayNotify.ts `task-assigned`: "<kişi> sana bir görev atadı" + "Görevi aç".
    pub(super) fn overlay_activity(&self, item: &p2_client_core::ActivityItem) {
        if self.overlay.active() == 0 || item.kind != "ASSIGNED" || !notify_prefs().tasks || item.actor.id == self.auth.user.id {
            return;
        }
        let Some(task) = item.task_id.clone() else { return };
        let actor = if item.actor.display_name.is_empty() { item.actor.username.clone() } else { item.actor.display_name.clone() };
        crate::trace(&format!("overlay: görev bildirimi {actor}"));
        self.overlay.toast(
            None,
            Toast {
                kind: "task".into(),
                title: format!("{actor} sana bir görev atadı"),
                text: item.preview.chars().take(140).collect(),
                tag: Some(format!("task:{task}")),
                avatar: Some(p2_overlay_client::ToastAvatar { user_id: item.actor.id.clone(), name: actor }),
                actions: vec![p2_overlay_client::ToastAction { id: "open-task".into(), label: "Görevi aç".into(), primary: true }],
                payload: Some(serde_json::json!({ "open": { "window": "tasks", "target": { "taskId": task, "projectId": item.project_id } } })),
                ..Default::default()
            },
        );
    }

    /// overlayNotify.ts `play` / `play-stop`: ekipten biri bir projeyi çalıştırdı / oyundan çıktı
    /// (`presence:activity` geçişi). `was`/`now`: önceki ve yeni etkinlik (kind, projectId, projectName).
    pub(super) fn overlay_presence_activity(&self, payload: &serde_json::Value) {
        let user = payload["userId"].as_str().unwrap_or_default().to_string();
        let was = LAST_ACTIVITY.lock().unwrap().insert(user.clone(), payload["activity"].clone());
        let was = was.as_ref();
        if self.overlay.active() == 0 || !notify_prefs().play || user.is_empty() || user == self.auth.user.id {
            return;
        }
        let playing = |v: Option<&serde_json::Value>| v.filter(|a| a["kind"] == "playing").map(|a| (a["projectId"].as_str().unwrap_or("").to_string(), a["projectName"].as_str().unwrap_or("").to_string()));
        let (before, now) = (playing(was), playing(Some(&payload["activity"])));
        let name = self.members.iter().chain(self.org_members.iter()).find(|m| m.id == user).map_or("Üye".to_string(), |m| m.display_name.clone());
        let avatar = Some(p2_overlay_client::ToastAvatar { user_id: user.clone(), name: name.clone() });
        let tag = Some(format!("play:{user}"));
        match (before, now) {
            (b, Some((pid, pname))) if b.as_ref().map(|x| &x.0) != Some(&pid) => {
                crate::trace(&format!("overlay: oyunda bildirimi {name} → {pname}"));
                self.overlay.toast(None, Toast { kind: "play".into(), title: name, text: format!("{pname} projesini çalıştırdı"), tag, avatar, duration_ms: Some(5000), actions: vec![p2_overlay_client::ToastAction { id: "open-overview".into(), label: "Özet".into(), primary: false }], payload: Some(serde_json::json!({ "open": { "window": "summary", "target": { "projectId": pid } } })), ..Default::default() });
            }
            (Some((_, pname)), None) => {
                crate::trace(&format!("overlay: oyundan çıktı bildirimi {name}"));
                self.overlay.toast(None, Toast { kind: "play-stop".into(), title: name, text: format!("{pname} oyunundan çıktı"), tag, avatar, duration_ms: Some(4000), ..Default::default() });
            }
            _ => {}
        }
    }

    pub(super) fn overlay_incoming(&self, call: &CallState, from_name: &str) {
        if self.overlay.active() == 0 || !notify_prefs().calls {
            return;
        }
        self.overlay.toast(
            None,
            Toast {
                kind: "call".into(),
                title: from_name.to_string(),
                text: match call.kind.as_str() {
                    "channel" | "CHANNEL" => "Seni sesli odaya çağırıyor",
                    "group" | "GROUP" => "Grup araması",
                    _ => "Seni arıyor",
                }
                .into(),
                sticky: true,
                tag: Some(format!("call:{}", call.id)),
                avatar: call.caller.clone().map(|id| p2_overlay_client::ToastAvatar { user_id: id, name: from_name.to_string() }),
                actions: vec![
                    p2_overlay_client::ToastAction { id: "accept".into(), label: "Kabul et".into(), primary: true },
                    p2_overlay_client::ToastAction { id: "decline".into(), label: "Reddet".into(), primary: false },
                    p2_overlay_client::ToastAction { id: "silence".into(), label: "Sustur".into(), primary: false },
                ],
                payload: Some(serde_json::json!({ "call": { "callId": call.id } })),
                ..Default::default()
            },
        );
    }

    pub(super) fn overlay_call_over(&self, call_id: &str) {
        if self.overlay.active() > 0 {
            self.overlay.dismiss(&format!("call:{call_id}"));
        }
    }

    /// overlayNotify.ts `release`: yeni sürüm yayınlandı / canlıya alındı. Kart gittiyse true
    /// (masaüstü bildirimi atlanır).
    pub(super) fn overlay_release(&self, project_id: &str, project_name: &str, version: &str, live: bool) -> bool {
        if self.overlay.active() == 0 || !notify_prefs().releases {
            return false;
        }
        let name = if project_name.is_empty() { "Kütüphane" } else { project_name };
        let text = if live { "Sürüm canlıya alındı." } else { "Yeni sürüm yayınlandı." };
        crate::trace(&format!("overlay: sürüm bildirimi {name} · {version} ({text})"));
        self.overlay.toast(None, Toast { kind: "release".into(), title: format!("{name} · {version}"), text: text.into(), tag: Some(format!("release:{project_id}")), ..Default::default() });
        true
    }

    /// overlayNotify.ts `download-done`: kütüphane indirmesi bitti.
    pub(super) fn overlay_download_done(&self, project_id: &str, project_name: &str) {
        if self.overlay.active() == 0 || !notify_prefs().downloads {
            return;
        }
        let name = if project_name.is_empty() { "Kütüphane" } else { project_name };
        crate::trace(&format!("overlay: indirme bildirimi {name}"));
        self.overlay.toast(None, Toast { kind: "download".into(), title: name.into(), text: "İndirme tamamlandı.".into(), tag: Some(format!("download:{project_id}")), ..Default::default() });
    }
}

impl WorkspaceScreen {
    /// Otomatik pilot: `P2_AUTOPILOT_OVERLAY_LOOK="<ms>|scale=1.25,opacity=0.6,hudpos=top-right,details=off,motion=reduce,hud=on,corner=top-left,messages=off"`
    /// — oyun hazır olduktan <ms> sonra görünüm ayarlarını değiştirir (ayarlar penceresindeki gibi
    /// kaydedilir ve açık oyuna anında iletilir).
    fn autopilot_overlay_look(&mut self, cx: &mut Context<Self>) {
        let Ok(spec) = std::env::var("P2_AUTOPILOT_OVERLAY_LOOK") else { return };
        let (delay, kv) = spec.split_once('|').unwrap_or(("3000", spec.as_str()));
        let delay = delay.parse().unwrap_or(3000u64);
        let kv = kv.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(delay)).await;
            let _ = this.update(cx, |this, cx| {
                let Some(app) = this.app.upgrade() else { return };
                app.update(cx, |app, _| {
                    let o = &mut app.settings.overlay;
                    for pair in kv.split(',') {
                        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                        match k {
                            "scale" => o.scale = v.parse().unwrap_or(1.),
                            "opacity" => o.opacity = v.parse().unwrap_or(0.92),
                            "hudpos" => o.hud_position = v.into(),
                            "details" => o.hud_details = v == "on",
                            "motion" => o.reduce_motion = v == "reduce",
                            "hud" => o.hud = v == "on",
                            "corner" => o.toast_corner = v.into(),
                            "messages" => o.notifications.messages = v == "on",
                            _ => {}
                        }
                    }
                    p2_client_core::store::save_settings(&app.settings);
                });
                crate::trace(&format!("overlay: görünüm ayarları değişti ({kv})"));
                this.apply_prefs(cx);
            });
        })
        .detach();
    }
}

/// Oyun içi bildirim türleri (Ayarlar > Kütüphane > Oyun içi overlay · Bildirimler); `apply_prefs` günceller.
static NOTIFY_PREFS: std::sync::Mutex<Option<p2_client_core::store::OverlayNotifications>> = std::sync::Mutex::new(None);

fn notify_prefs() -> p2_client_core::store::OverlayNotifications {
    NOTIFY_PREFS.lock().unwrap().clone().unwrap_or_default()
}

impl WorkspaceScreen {
    /// Ayarlardaki kütüphane/overlay/uygunluk tercihlerini uygular (açılışta ve her değişiklikte).
    pub(super) fn apply_prefs(&mut self, cx: &mut Context<Self>) {
        let Some(app) = self.app.upgrade() else { return };
        let s = app.read(cx).settings.clone();
        let h = &s.overlay.hotkeys;
        let list = p2_overlay_client::hotkeys::to_native(&h.panel, &h.screenshot, &h.mute, &h.deafen, &h.push_to_talk);
        self.overlay.set_hotkeys(list);
        self.overlay.set_enabled(s.library.overlay);
        self.overlay.set_shot_dir(&std::env::var("P2_SHOT_DIR").unwrap_or(s.overlay.screenshot_dir.clone()));
        let o = &s.overlay;
        *NOTIFY_PREFS.lock().unwrap() = Some(o.notifications.clone());
        self.overlay.set_look(p2_overlay_client::Look {
            hud: o.hud,
            hud_position: o.hud_position.clone(),
            hud_details: o.hud_details,
            toast_corner: o.toast_corner.clone(),
            opacity: o.opacity,
            scale: o.scale,
            reduce_motion: o.reduce_motion,
        });
        self.patcher.set_limit_mbps(s.library.bandwidth_mbps);
        crate::trace(&format!("tercihler uygulandı: overlay={} hız={} kısayol paneli={}", s.library.overlay, s.library.bandwidth_mbps, h.panel));
        self.check_availability(cx);
    }

    /// Uygunluk: çalışma saatleri dışında durumu otomatik Uzakta/Meşgul yapar, saat içine
    /// dönünce (yalnız kendisi değiştirdiyse) Çevrimiçi'ne geri alır.
    pub(super) fn check_availability(&mut self, cx: &mut Context<Self>) {
        use chrono::{Datelike, Timelike};
        let Some(app) = self.app.upgrade() else { return };
        let av = app.read(cx).settings.availability.clone();
        // Otomatik pilot (testler) gerçek saate göre durumu değiştirmez; yalnız P2_FAKE_CLOCK ile.
        if av.outside_hours_status == "none" || (std::env::var_os("P2_AUTOPILOT_SERVER").is_some() && std::env::var_os("P2_FAKE_CLOCK").is_none()) {
            return;
        }
        let now = chrono::Local::now();
        let working = match std::env::var("P2_FAKE_CLOCK").ok().and_then(|v| v.split_once(' ').map(|(d, t)| (d.parse::<u8>().ok(), t.to_string()))) {
            Some((Some(day), t)) => {
                let m = t.split_once(':').and_then(|(h, m)| Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)).unwrap_or(0);
                av.working(day, m)
            }
            _ => av.working(now.weekday().num_days_from_sunday() as u8, now.hour() * 60 + now.minute()),
        };
        let status = if av.outside_hours_status == "BUSY" { "BUSY" } else { "AWAY" };
        // Saat dışı: çevrimiçiyse ya da tercih değiştiyse (kendi koyduğumuz durumu güncelle).
        if !working && ((!self.auto_away && self.preference == "ONLINE") || (self.auto_away && self.preference != status)) {
            self.auto_away = true;
            crate::trace(&format!("uygunluk: çalışma saati dışı → {status}"));
            self.choose_presence(status, cx);
        } else if working && self.auto_away {
            self.auto_away = false;
            crate::trace("uygunluk: çalışma saati → ONLINE");
            self.choose_presence("ONLINE", cx);
        }
    }
}
static LAST_ACTIVITY: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>> = std::sync::LazyLock::new(Default::default);
