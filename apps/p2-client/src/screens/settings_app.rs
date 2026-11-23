//! Ayarlar > Bildirimler (`NotificationSection`) ve Ayarlar > Uygulama (`AppSection`: kapat düğmesi,
//! görev ayrıntısı, sürüm ve güncelleme, arama balonu, sunucu adresi, bu bilgisayardaki sunucu).

use gpui_kit::component::input::InputState;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use p2_client_core::store::{self, AppSettings};
use p2_ui::tokens::{Palette, font_size, fs, radius};

use super::settings_ui::{Btn, badge, block, button, choice, field, hint, last_block, row, slider, toggle_row};
use super::{SettingsModal, sliders};

/// Kapat düğmesi seçenekleri (SettingsModal.tsx `AppSection` sırası; metinler `app.*`).
pub const CLOSE_ACTIONS: [(&str, &str, &str); 4] = [("tray", "app.tray", "app.tray.hint"), ("minimize", "app.minimize", "app.minimize.hint"), ("quit", "app.quit", "app.quit.hint"), ("ask", "app.ask", "app.ask.hint")];

/// "Bu bilgisayardaki sunucu" (main.cjs `server:status` / `server:service-status`).
#[derive(Default, Clone)]
pub struct ServerState {
    pub loaded: bool,
    pub configured: bool,
    pub alive: bool,
    pub registered: bool,
    pub port: u16,
    pub busy: &'static str,
    pub polling: bool,
}

fn server_data_dir() -> std::path::PathBuf {
    p2_client_core::paths::shared_data_dir().join("P2")
}

fn installer_dir() -> std::path::PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default()
}

/// Durum yoklaması (yalnız okur: server.json, /health, `sc query` / `schtasks /Query`).
fn probe_server() -> ServerState {
    let port: u16 = std::env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(4000);
    let configured = std::fs::read_to_string(server_data_dir().join("server.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(t.trim_start_matches('\u{feff}')).ok()).is_some();
    let alive = {
        use std::io::{Read, Write};
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(1200)).ok().and_then(|mut s| {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(1200)));
            s.write_all(format!("GET /health HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).ok()?;
            let mut body = String::new();
            let _ = s.read_to_string(&mut body);
            Some(body.contains("\"status\":\"ok\"") || body.contains("\"status\": \"ok\""))
        }) == Some(true)
    };
    let quiet = |exe: &str, args: &[&str]| {
        let mut cmd = std::process::Command::new(exe);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
        cmd.args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success())
    };
    // Kayıtlı hizmet (Windows hizmeti / zamanlanmış görev) yalnız Windows'ta.
    let registered = cfg!(windows) && configured && (quiet("sc.exe", &["query", "P2Server"]) || quiet("schtasks", &["/Query", "/TN", "P2 Server"]));
    ServerState { loaded: true, configured, alive, registered, port, busy: "", polling: true }
}

impl SettingsModal {
    pub(super) fn update_app(&mut self, change: impl FnOnce(&mut AppSettings), cx: &mut Context<Self>) {
        let Some(mut current) = self.app.upgrade().map(|app| app.read(cx).settings.app.clone()) else { return };
        change(&mut current);
        crate::trace(&format!("ayarlar: uygulama kapat={} bildirim={} ses={} görev={}", current.close_action, current.desktop_notifications, current.notification_sound, current.task_view));
        crate::set_desktop_notifications(current.desktop_notifications);
        let _ = self.app.update(cx, |app, _| {
            app.settings.app = current;
            store::save_settings(&app.settings);
        });
        cx.notify();
    }

    pub(super) fn notifications(&mut self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (a, ring) = self.app.upgrade().map(|app| {
            let s = &app.read(cx).settings;
            (s.app.clone(), s.media.ringtone_volume)
        }).unwrap_or_default();
        let volume = slider("notify-volume", p2_i18n::t("notify.volume"), ring as i32, format!("{ring}%"), 0, 100, 1, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| this.update_media(move |m| m.ringtone_volume = v as u32, cx)), cx);
        div()
            .flex()
            .flex_col()
            .child(
                block(&p2_i18n::t("notify.title"), p)
                    .child(toggle_row("n-sound", p2_i18n::t("notify.sound"), Some(p2_i18n::t("notify.sound.hint").into()), a.notification_sound, self.epoch("n-sound"), p).on_click(cx.listener(|this, _, _, cx| {
                        this.bump("n-sound");
                        this.update_app(|a| a.notification_sound = !a.notification_sound, cx)
                    })))
                    .child(toggle_row("n-desktop", p2_i18n::t("notify.desktop"), Some(p2_i18n::t("notify.desktop.hint").into()), a.desktop_notifications, self.epoch("n-desktop"), p).on_click(cx.listener(|this, _, _, cx| {
                        this.bump("n-desktop");
                        this.update_app(|a| a.desktop_notifications = !a.desktop_notifications, cx)
                    })))
                    .child(volume)
                    .child(div().flex().child(button("n-test", p2_i18n::t("notify.test"), Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| {
                        // notifications.ts playMessageSound(true): kısa mesaj işareti (testlerde aygıt açılmaz).
                        crate::trace("ayarlar: bildirim sesi denendi");
                        this.calls.update(cx, |c, cx| c.play_cue(p2_media::Cue::Message, window, cx));
                    })))),
            )
            .child(hint(p2_i18n::t("notify.hint"), p))
            .into_any_element()
    }

    pub(super) fn application(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let a = self.app.upgrade().map(|app| app.read(cx).settings.app.clone()).unwrap_or_default();
        let mut close = div().flex().flex_col().gap(px(6.));
        for (value, label, detail) in CLOSE_ACTIONS {
            close = close.child(choice(SharedString::from(format!("close-{value}")), p2_i18n::t(label), p2_i18n::t(detail), a.close_action == value, p).on_click(cx.listener(move |this, _, _, cx| this.update_app(|a| a.close_action = value.to_string(), cx))));
        }
        let mut task = div().flex().flex_col().gap(px(6.));
        for (value, label, detail) in [("panel", "set2.task.panel", "set2.task.panel.hint"), ("center", "set2.task.center", "set2.task.center.hint")] {
            task = task.child(choice(SharedString::from(format!("task-{value}")), p2_i18n::t(label), p2_i18n::t(detail), a.task_view == value, p).on_click(cx.listener(move |this, _, _, cx| this.update_app(|a| a.task_view = value.to_string(), cx))));
        }

        // Arama balonu (SettingsModal `CallWidgetSetting`): yerel onay kutusu + metin.
        let widget_on = !crate::call_widget::load_state().hidden;
        let (hb, hbd) = (p.bg_hover, p.border_strong);
        let widget = block(&p2_i18n::t("set2.callWidget"), p)
            .child(hint(p2_i18n::t("set2.callWidget.hint"), p))
            .child(
                div()
                    .id("call-widget")
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(14.))
                    .px(px(11.))
                    .py(px(9.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::MD)
                    .bg(p.bg_3)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hb).border_color(hbd))
                    .child(div().mx(px(4.)).my(px(3.)).child(p2_ui::form::native_checkbox(widget_on, p)))
                    .child(div().text_color(p.text).line_height(px(16.)).child(p2_i18n::t("set2.callWidget.show")))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        crate::call_widget::save_state(|s| s.hidden = widget_on);
                        crate::trace(&format!("ayarlar: arama balonu {}", if widget_on { "kapalı" } else { "açık" }));
                        cx.notify();
                    })),
            );

        // Sunucu adresi.
        let current = self.app.upgrade().map(|a| a.read(cx).settings.api_url.clone()).unwrap_or_default();
        if self.server_addr.is_none() {
            let cur = current.clone();
            // ServerAddress: değer = kayıtlı adres (p2:apiUrl; bağlanınca hep yazılır), yer tutucu = kullanılan adres.
            let state = cx.new(|cx| InputState::new(window, cx).placeholder(cur.clone()));
            state.update(cx, |s, cx| s.set_value(cur, window, cx));
            self.server_addr = Some(state);
        }
        let addr = self.server_addr.clone().unwrap();
        let address = |last: bool, this: &Self, window: &Window, cx: &mut Context<Self>| {
            let b = if last { last_block(&p2_i18n::t("app.server"), p) } else { block(&p2_i18n::t("app.server"), p) };
            b.child(field(p2_i18n::t("app.address"), this.input(&addr, p, window, cx), p))
                .child(hint(p2_i18n::tv("set2.address.hint", &[("current", &current)]), p))
                .child(row().child(button("addr-apply", p2_i18n::t("app.applyReload"), Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| {
                    let value = this.server_addr.as_ref().map(|i| super::super::value(i, cx).trim().to_string()).unwrap_or_default();
                    let Some(app) = this.app.upgrade() else { return };
                    let url = if value.is_empty() { app.read(cx).settings.api_url.clone() } else { value };
                    crate::trace(&format!("ayarlar: sunucu adresi {url}"));
                    cx.emit(super::SettingsEvent::Close);
                    app.update(cx, |app, cx| app.connected(url, window, cx));
                }))))
        };

        // Bu bilgisayardaki sunucu (yalnız yapılandırılmışsa).
        if !self.server.polling {
            self.server.polling = true;
            cx.spawn(async move |this, cx| {
                loop {
                    let state = cx.background_executor().spawn(async { probe_server() }).await;
                    let alive = this.update(cx, |this, cx| {
                        if !this.server.loaded {
                            crate::trace(&format!("ayarlar: sunucu durumu okundu (yapılandırılmış={})", state.configured));
                        }
                        let busy = this.server.busy;
                        this.server = ServerState { busy, ..state };
                        cx.notify();
                    });
                    if alive.is_err() {
                        break;
                    }
                    cx.background_executor().timer(std::time::Duration::from_secs(5)).await;
                }
            })
            .detach();
        }
        let s = self.server.clone();
        let service = s.configured.then(|| {
            let busy = !s.busy.is_empty();
            last_block(&p2_i18n::t("set2.server.title"), p)
                .child(row().child(badge(if s.alive { p2_i18n::tv("set2.server.running", &[("port", &s.port.to_string())]) } else { p2_i18n::t("set2.server.stopped") }, s.alive, p)).child(badge(p2_i18n::t(if s.registered { "set2.server.registered" } else { "set2.server.unregistered" }), s.registered, p)))
                .child(
                    div()
                        .text_size(fs(10.))
                        .line_height(gpui_kit::relative(1.5))
                        .text_color(p.text_3)
                        .child({ let t = p2_i18n::t(if s.registered { "set2.server.info.registered" } else { "set2.server.info.unregistered" }); let at = t.find("P2 Server").unwrap_or(0); gpui_kit::StyledText::new(t).with_highlights([(at..at + 9, gpui_kit::HighlightStyle { font_weight: Some(FontWeight::BOLD), ..Default::default() })]) }),
                )
                .child(
                    row()
                        .when(!s.registered, |r| {
                            r.child(button("srv-service", p2_i18n::t(if s.busy == "service" { "set2.server.registering" } else { "set2.server.register" }), Btn::Default, busy, p).on_click(cx.listener(|this, _, _, cx| {
                                if !this.server.busy.is_empty() {
                                    return;
                                }
                                this.server.busy = "service";
                                cx.notify();
                                let script = installer_dir().join("p2-server-install.bat");
                                crate::trace(&format!("ayarlar: servis betiği {}", script.display()));
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor()
                                        .spawn(async move {
                                            if script.exists() {
                                                let _ = std::process::Command::new("cmd.exe").args(["/c", "start", "/wait", ""]).arg(&script).status();
                                            }
                                        })
                                        .await;
                                    cx.background_executor().timer(std::time::Duration::from_secs(3)).await;
                                    let _ = this.update(cx, |this, cx| {
                                        this.server.busy = "";
                                        cx.notify();
                                    });
                                })
                                .detach();
                            })))
                        })
                        .when(!s.alive, |r| {
                            r.child(button("srv-start", p2_i18n::t(if s.busy == "start" { "set2.server.starting" } else { "set2.server.start" }), Btn::Default, busy, p).on_click(cx.listener(|this, _, _, cx| {
                                if !this.server.busy.is_empty() {
                                    return;
                                }
                                // Rust istemcisi sunucu taşımaz (P2 Server ayrı uygulama): kayıtlı hizmeti başlatmayı dener.
                                this.server.busy = "start";
                                cx.notify();
                                crate::trace("ayarlar: sunucu başlatılıyor");
                                cx.spawn(async move |this, cx| {
                                    cx.background_executor()
                                        .spawn(async {
                                            #[cfg(windows)]
                                            {
                                                use std::os::windows::process::CommandExt;
                                                let _ = std::process::Command::new("sc.exe").args(["start", "P2Server"]).creation_flags(0x0800_0000).status();
                                            }
                                        })
                                        .await;
                                    cx.background_executor().timer(std::time::Duration::from_secs(4)).await;
                                    let _ = this.update(cx, |this, cx| {
                                        this.server.busy = "";
                                        cx.notify();
                                    });
                                })
                                .detach();
                            })))
                        })
                        .child(button("srv-folder", p2_i18n::t("set2.server.openFolder"), Btn::Default, false, p).on_click(cx.listener(|_, _, _, _| {
                            let script = installer_dir().join("p2-server-install.bat");
                            let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", script.display())).spawn();
                        }))),
                )
        });

        let mut col = div()
            .flex()
            .flex_col()
            .child(block(&p2_i18n::t("app.onClose"), p).child(close))
            .child(block(&p2_i18n::t("set2.task.title"), p).child(hint(p2_i18n::t("set2.task.hint"), p)).child(task))
            .child(self.update_block(p, cx))
            .child(widget)
            .child(address(service.is_none(), self, window, cx));
        if let Some(service) = service {
            col = col.child(service);
        }
        col.into_any_element()
    }

    /// SettingsModal `UpdateSetting`: "Sürüm ve güncelleme" — sürüm rozeti, canlı durum,
    /// "Güncellemeleri denetle" (2,5 sn sonra not), hazırsa "Yeniden başlat ve kur".
    fn update_block(&mut self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        crate::updates::watch(cx);
        let s = crate::updates::state();
        let (checking, note) = {
            let ui = crate::updates::check_ui().lock().unwrap();
            (ui.checking, ui.note.clone())
        };
        let current = if s.current.is_empty() { crate::updates::product_version().to_string() } else { s.current.clone() };
        let ready = s.status == p2_updater::Status::Ready;
        let note = if !s.error.is_empty() { s.error.clone() } else { note };
        block(&p2_i18n::t("set2.update.title"), p)
            .child(hint(p2_i18n::t("set2.update.hint"), p))
            .child(
                row()
                    .child(badge(p2_i18n::tv("set2.update.version", &[("version", &current)]), true, p))
                    .child(badge(crate::updates::status_label(&s), ready, p))
                    .child(button("upd-check", p2_i18n::t(if checking { "set2.update.checking" } else { "set2.update.check" }), Btn::Default, checking, p).on_click(cx.listener(|_, _, _, cx| {
                        crate::updates::check_from_settings();
                        cx.notify();
                    })))
                    .when(ready, |d| {
                        d.child(button("upd-install", p2_i18n::t("set2.update.install"), Btn::Default, false, p).on_click(cx.listener(|_, _, _, _| {
                            crate::trace("ayarlar: yeniden başlat ve kur");
                            crate::updates::install();
                        })))
                    }),
            )
            .when(!note.is_empty(), |d| d.child(hint(note, p)))
            .into_any_element()
    }
}

#[allow(dead_code)]
fn _sizes() -> gpui_kit::Pixels {
    font_size::XS
}
