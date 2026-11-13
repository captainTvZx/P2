//! Ayarlar > Kütüphane (kurulum konumu, indirme, oyun ve etkinlik, oyun içi overlay: kısayollar,
//! görünüm, bildirimler, ekran görüntüsü klasörü) ve Ayarlar > Uygunluk (çalışma günleri/saatleri,
//! saat dışında otomatik durum). SettingsModal.tsx `LibrarySection`, `OverlaySettings`,
//! `AvailabilitySection`; kısayol satırı `src/ui/HotkeyInput.tsx`.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, FontWeight, Keystroke, SharedString, Window, div, px, relative};
use p2_client_core::store::{self, Availability, LibraryPrefs, OverlayPrefs};
use p2_overlay_client::hotkeys;
use p2_ui::controls::{BtnSize, BtnVariant, kbd};
use p2_ui::tokens::{Palette, css_rgba, font_size, fs, radius, shadows};

use super::settings_ui::{Btn, block, button, choice, field, hint, last_block, row, select, select_menu, slider, text, toggle_row};
use super::{SettingsEvent, SettingsModal, sliders};

const CORNERS: [(&str, &str); 4] = [("top-right", "Sağ üst"), ("top-left", "Sol üst"), ("bottom-right", "Sağ alt"), ("bottom-left", "Sol alt")];
const HUD_POS: [(&str, &str); 3] = [("top-left", "Sol üst"), ("top-center", "Üst orta"), ("top-right", "Sağ üst")];
const BANDWIDTH: [(u64, &str); 6] = [(0, "Sınırsız"), (5, "5 MB/s"), (10, "10 MB/s"), (25, "25 MB/s"), (50, "50 MB/s"), (100, "100 MB/s")];

/// Diskteki boş alan (kütüphane kökünün sürücüsü).
fn free_space(path: &str) -> Option<u64> {
    use windows::core::HSTRING;
    let mut free = 0u64;
    unsafe { windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(&HSTRING::from(path), Some(&mut free), None, None) }.ok()?;
    Some(free)
}

impl SettingsModal {
    fn prefs(&self, cx: &Context<Self>) -> (LibraryPrefs, OverlayPrefs, Availability) {
        self.app.upgrade().map(|a| {
            let s = &a.read(cx).settings;
            (s.library.clone(), s.overlay.clone(), s.availability.clone())
        }).unwrap_or_default()
    }

    /// Tercihi değiştir, kaydet, çalışma alanına bildir.
    pub(super) fn update_prefs(&mut self, change: impl FnOnce(&mut LibraryPrefs, &mut OverlayPrefs, &mut Availability), cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, _| {
            let s = &mut app.settings;
            change(&mut s.library, &mut s.overlay, &mut s.availability);
            store::save_settings(s);
            crate::trace(&format!(
                "ayarlar: kütüphane hız={} overlay={} hud={} köşe={} kısayollar={}/{}/{} uygunluk={:?} {}-{} {}",
                s.library.bandwidth_mbps,
                s.library.overlay,
                s.overlay.hud,
                s.overlay.toast_corner,
                s.overlay.hotkeys.panel,
                s.overlay.hotkeys.screenshot,
                s.overlay.hotkeys.mute,
                s.availability.work_days,
                s.availability.work_start,
                s.availability.work_end,
                s.availability.outside_hours_status
            ));
        });
        cx.emit(SettingsEvent::PrefsChanged);
        cx.notify();
    }

    /// Yakalanan tuş kısayol olur (Esc vazgeçer, Backspace boşaltır — boş bırakılabilen eylemde;
    /// yalnız değiştirici basılırsa beklemeye devam).
    pub(super) fn capture_hotkey(&mut self, k: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(name) = self.capturing_hotkey else { return false };
        crate::trace(&format!("ayarlar: tuş yakalandı {} ({name})", k.key));
        if k.key == "escape" {
            self.capturing_hotkey = None;
            cx.notify();
            return true;
        }
        let empty_ok = matches!(name, "deafen" | "pushToTalk");
        if k.key == "backspace" {
            self.capturing_hotkey = None;
            if empty_ok {
                self.set_hotkey(name, String::new(), cx);
            }
            cx.notify();
            return true;
        }
        let Some(value) = hotkeys::from_gpui(&k.key, k.modifiers.control, k.modifiers.alt, k.modifiers.shift) else { return true };
        self.capturing_hotkey = None;
        self.set_hotkey(name, value, cx);
        true
    }

    pub fn set_hotkey(&mut self, name: &str, value: String, cx: &mut Context<Self>) {
        let name = name.to_string();
        self.update_prefs(
            move |_, o, _| match name.as_str() {
                "panel" => o.hotkeys.panel = value,
                "screenshot" => o.hotkeys.screenshot = value,
                "mute" => o.hotkeys.mute = value,
                "deafen" => o.hotkeys.deafen = value,
                _ => o.hotkeys.push_to_talk = value,
            },
            cx,
        );
    }

    fn pick_library_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Kütüphane konumu".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.first() {
                    let root = path.to_string_lossy().to_string();
                    let free = free_space(&root);
                    let _ = this.update(cx, |this, cx| {
                        this.library_root = root.clone();
                        this.library_free = free;
                        cx.emit(SettingsEvent::LibraryRoot(root));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn pick_shot_dir(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Ekran görüntüsü klasörü".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.first() {
                    let dir = path.to_string_lossy().to_string();
                    let _ = this.update(cx, |this, cx| this.update_prefs(move |_, o, _| o.screenshot_dir = dir, cx));
                }
            }
        })
        .detach();
    }

    fn pref_select(&self, name: &'static str, options: Vec<SharedString>, selected: usize, p: &Palette, cx: &mut Context<Self>, pick: impl Fn(&mut Self, usize, &mut Context<Self>) + 'static) -> gpui_kit::Div {
        let open = self.open_select == Some(name);
        div()
            .child(select(SharedString::from(format!("sel-{name}")), options[selected].clone(), open, p).on_click(cx.listener(move |this, _, _, cx| this.toggle_select(name, cx))))
            .when(open, |s| {
                s.child(select_menu(name, &options, selected, self.content_w, p, cx, move |this: &mut Self, i, _, cx| {
                    this.open_select = None;
                    pick(this, i, cx);
                }))
            })
    }

    /// HotkeyInput.tsx: etiket + tuş çipleri (ya da "Kapalı") + "Tuşu değiştir"; dinlerken vurgulu.
    fn hotkey_row(&self, name: &'static str, value: &str, conflict: Option<&str>, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let label = hotkeys::LABELS.iter().find(|l| l.0 == name).map(|l| l.1).unwrap_or(name);
        let listening = self.capturing_hotkey == Some(name);
        let empty_ok = matches!(name, "deafen" | "pushToTalk");
        let chips = hotkeys::format_keys(value);
        let value_el: AnyElement = if listening {
            div().text_size(font_size::XS).text_color(p.text_strong).child(format!("Bir tuşa bas… (Esc vazgeç{})", if empty_ok { ", Backspace temizle" } else { "" })).into_any_element()
        } else if chips.is_empty() {
            div().text_size(font_size::XS).text_color(p.text_3).child("Kapalı").into_any_element()
        } else {
            // `.p2-keys`: çipler arası 3px; "+" sonraki çiple aynı kutuda (1px kenar).
            let mut keys = div().flex().items_center().gap(px(3.));
            for (i, k) in chips.iter().enumerate() {
                if i == 0 {
                    keys = keys.child(kbd(k.clone(), p));
                } else {
                    keys = keys.child(div().flex().items_center().child(div().mx(px(1.)).text_size(font_size::XS).text_color(css_rgba(255, 255, 255, 0.4)).child("+")).child(kbd(k.clone(), p)));
                }
            }
            keys.into_any_element()
        };
        let hover = p.border_strong;
        div()
            .w_full()
            .min_h(px(38.))
            .px(px(10.))
            .py(px(4.))
            .flex()
            .flex_col()
            .justify_center()
            .border_1()
            .rounded(radius::SM)
            .map(|s| {
                if listening {
                    s.border_color(p.border_accent).bg(p.accent_soft).shadow(shadows(&[p.ring]))
                } else if conflict.is_some() {
                    s.border_color(p.danger_border).bg(p.bg_3)
                } else {
                    s.border_color(p.border).bg(p.bg_3).hover(move |s| s.border_color(hover))
                }
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().flex_1().min_w_0().truncate().text_size(font_size::SM).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(label))
                    .child(div().min_w(px(110.)).flex().justify_end().child(value_el))
                    .child(
                        p2_ui::controls::button(SharedString::from(format!("hk-{name}")), BtnVariant::Secondary, BtnSize::Sm, false, false, p)
                            .child(if listening { "Bekleniyor" } else { "Tuşu değiştir" })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.capturing_hotkey = if this.capturing_hotkey == Some(name) { None } else { Some(name) };
                                window.focus(&this.focus, cx);
                                cx.notify();
                            })),
                    ),
            )
            .when_some(conflict.filter(|_| !listening), |s, other| s.child(div().text_size(font_size::XXS).text_color(p.danger_text).child(format!("Bu tuş zaten '{other}' için kullanılıyor"))))
            .into_any_element()
    }

    pub(super) fn library(&mut self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (lib, ov, _) = self.prefs(cx);
        // Kurulum konumu
        let has_root = !self.library_root.is_empty();
        let root = if has_root { self.library_root.clone() } else { "Henüz seçilmedi".to_string() };
        let location = block("Kurulum konumu", p)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .text_size(fs(10.))
                    .line_height(relative(1.5))
                    .text_color(p.text_3)
                    .child("Oyunlar seçtiğin diskte\u{a0}")
                    .child(div().font_family(p2_ui::tokens::FONT_MONO).child("P2ProjectLibrary"))
                    .child("\u{a0}klasörüne kurulur."),
            )
            .child(
                row()
                    .items_center()
                    .gap(px(12.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(p.text).child(root))
                    .when_some(self.library_free, |r, free| r.child(hint(format!("{} boş", fmt_bytes(free)), p))),
            )
            .child(div().flex().child(button("lib-root", if has_root { "Değiştir" } else { "Konum seç" }, Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| this.pick_library_root(window, cx)))));

        // İndirme
        let bw_ix = BANDWIDTH.iter().position(|b| b.0 == lib.bandwidth_mbps).unwrap_or(0);
        let bw = self.pref_select("bandwidth", BANDWIDTH.iter().map(|b| SharedString::from(b.1)).collect(), bw_ix, p, cx, |this, i, cx| {
            let v = BANDWIDTH[i].0;
            this.update_prefs(move |l, _, _| l.bandwidth_mbps = v, cx)
        });
        let download = block("İndirme", p)
            .child(toggle_row("lib-auto", "Otomatik indir", Some("Yeni sürüm yayınlanınca kurulu oyunları kendiliğinden güncelle (Steam gibi)".into()), lib.auto_download, self.epoch("lib-auto"), p).on_click(cx.listener(|this, _, _, cx| {
                this.bump("lib-auto");
                this.update_prefs(|l, _, _| l.auto_download = !l.auto_download, cx)
            })))
            .child(div().mt(px(10.)).child(field("İndirme hız sınırı", bw, p)));

        // Oyun ve etkinlik
        let panel_keys = hotkeys::format_keys(&ov.hotkeys.panel).join("+");
        let overlay_label = format!("Oyun içi overlay ({})", if panel_keys.is_empty() { "kısayol yok".to_string() } else { panel_keys });
        let toggles: [(&'static str, String, &str, bool, fn(&mut LibraryPrefs)); 5] = [
            ("lib-overlay", overlay_label, "Oyunun üstünde P2 paneli: sesli oda, ekip, sohbet. Anti-cheat'li build'lerde kendiliğinden kapalı kalır", lib.overlay, |l| l.overlay = !l.overlay),
            ("lib-min", "Oyun açılınca P2'yi küçült".into(), "Oyunu başlatınca pencere simge durumuna iner", lib.minimize_on_launch, |l| l.minimize_on_launch = !l.minimize_on_launch),
            ("lib-share", "Etkinliğimi ekiple paylaş".into(), "Oynadığın, indirdiğin ve üzerinde çalıştığın proje ekibe görünür", lib.share_activity, |l| l.share_activity = !l.share_activity),
            ("lib-announce", "Çalıştırdığımı ekibe duyur".into(), "Oyunu çalıştırınca çevrimiçi ekip üyelerine Steam tarzı köşe bildirimi gider (gizlilik)", lib.announce_play, |l| l.announce_play = !l.announce_play),
            ("lib-toasts", "Ekipten biri çalıştırınca bildir".into(), "Biri bir projeyi çalıştırdığında sağ altta kısa bildirim göster", lib.play_toasts, |l| l.play_toasts = !l.play_toasts),
        ];
        let mut game = block("Oyun ve etkinlik", p);
        for (id, label, detail, on, flip) in toggles {
            game = game.child(toggle_row(id, label, Some(detail.into()), on, self.epoch(id), p).on_click(cx.listener(move |this, _, _, cx| {
                this.bump(id);
                this.update_prefs(move |l, _, _| flip(l), cx)
            })));
        }

        // Kısayollar
        let rows: [(&'static str, String, bool); 5] = [("panel", ov.hotkeys.panel.clone(), false), ("screenshot", ov.hotkeys.screenshot.clone(), false), ("mute", ov.hotkeys.mute.clone(), false), ("deafen", ov.hotkeys.deafen.clone(), true), ("pushToTalk", ov.hotkeys.push_to_talk.clone(), true)];
        let clash = hotkeys::conflict(&rows.iter().map(|(k, v, _)| (*k, v.as_str())).collect::<Vec<_>>());
        let label_of = |n: &str| hotkeys::LABELS.iter().find(|l| l.0 == n).map(|l| l.1).unwrap_or("");
        let mut list = div().flex().flex_col().gap(px(6.));
        for (name, value, _) in &rows {
            let other = clash.as_ref().and_then(|(a, b)| if a == name { Some(label_of(b)) } else if b == name { Some(label_of(a)) } else { None });
            list = list.child(self.hotkey_row(name, value, other, p, cx));
        }
        let keys = block("Oyun içi overlay · Kısayollar", p).child(hint("Kısayollar oyun odaktayken çalışır. Bas-konuş tuşu oyuna da iletilir; diğerleri yalnız P2'ye gider.", p)).child(list);

        // Görünüm
        let corner_ix = CORNERS.iter().position(|c| c.0 == ov.toast_corner).unwrap_or(0);
        let corner = self.pref_select("corner", CORNERS.iter().map(|c| SharedString::from(c.1)).collect(), corner_ix, p, cx, |this, i, cx| {
            let v = CORNERS[i].0;
            this.update_prefs(move |_, o, _| o.toast_corner = v.into(), cx)
        });
        let hud_ix = HUD_POS.iter().position(|c| c.0 == ov.hud_position).unwrap_or(0);
        let hud_pos = self.pref_select("hudpos", HUD_POS.iter().map(|c| SharedString::from(c.1)).collect(), hud_ix, p, cx, |this, i, cx| {
            let v = HUD_POS[i].0;
            this.update_prefs(move |_, o, _| o.hud_position = v.into(), cx)
        });
        let opacity = (ov.opacity * 100.).round() as i32;
        let scale = (ov.scale * 100.).round() as i32;
        let look_toggle = |this: &Self, id: &'static str, label: &'static str, detail: &'static str, on: bool, flip: fn(&mut OverlayPrefs), cx: &mut Context<Self>| {
            toggle_row(id, label, Some(detail.into()), on, this.epoch(id), p).on_click(cx.listener(move |this, _, _, cx| {
                this.bump(id);
                this.update_prefs(move |_, o, _| flip(o), cx)
            }))
        };
        let look = block("Oyun içi overlay · Görünüm", p)
            .child(field("Bildirim köşesi", corner, p))
            .child(slider("ov-opacity", "Pencere saydamlığı", opacity, format!("{opacity}%"), 35, 100, 1, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| this.update_prefs(move |_, o, _| o.opacity = v as f32 / 100., cx)), cx))
            .child(slider("ov-scale", "Arayüz ölçeği", scale, format!("{scale}%"), 80, 150, 5, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| this.update_prefs(move |_, o, _| o.scale = v as f32 / 100., cx)), cx))
            .child(look_toggle(self, "ov-motion", "Animasyonları azalt", "Overlay'de geçiş ve hareket efektlerini kapatır", ov.reduce_motion, |o| o.reduce_motion = !o.reduce_motion, cx))
            .child(look_toggle(self, "ov-hud", "FPS ve ping göstergesi", "Oyunun köşesinde kare hızı ve sesli oda gecikmesi", ov.hud, |o| o.hud = !o.hud, cx))
            .child(field("Gösterge yeri", hud_pos, p))
            .child(look_toggle(self, "ov-details", "Ayrıntılı gösterge", "Oyunun CPU'su, RAM, GPU ve paket kaybı da göstergede yazsın", ov.hud_details, |o| o.hud_details = !o.hud_details, cx));

        let note_rows: [(&'static str, &str, &str, bool, fn(&mut OverlayPrefs)); 6] = [
            ("on-msg", "Mesaj ve etiket bildirimleri", "Özel mesaj, grup ve @etiket kartları oyun içinde görünür", ov.notifications.messages, |o| o.notifications.messages = !o.notifications.messages),
            ("on-call", "Arama ve oda davetleri", "Gelen aramayı oyun içinden kabul edebilir ya da reddedebilirsin", ov.notifications.calls, |o| o.notifications.calls = !o.notifications.calls),
            ("on-play", "Ekipten biri oyuna girince/çıkınca", "Ekip arkadaşların oyuna girip çıktığında kısa kart", ov.notifications.play, |o| o.notifications.play = !o.notifications.play),
            ("on-task", "Bana görev atanınca", "Atanan görev oyun içinden açılabilir", ov.notifications.tasks, |o| o.notifications.tasks = !o.notifications.tasks),
            ("on-rel", "Sürüm yayınlanınca", "Yeni sürüm yayınlandığında ya da canlıya alındığında bildir", ov.notifications.releases, |o| o.notifications.releases = !o.notifications.releases),
            ("on-dl", "İndirme bitince", "Kütüphane indirmesi tamamlanınca bildir", ov.notifications.downloads, |o| o.notifications.downloads = !o.notifications.downloads),
        ];
        let mut notes = block("Oyun içi overlay · Bildirimler", p);
        for (id, label, detail, on, flip) in note_rows {
            notes = notes.child(toggle_row(id, label, Some(detail.into()), on, self.epoch(id), p).on_click(cx.listener(move |this, _, _, cx| {
                this.bump(id);
                this.update_prefs(move |_, o, _| flip(o), cx)
            })));
        }

        let custom_dir = !ov.screenshot_dir.is_empty();
        let shots = last_block("Ekran görüntüsü klasörü", p)
            .child(
                row()
                    .items_center()
                    .gap(px(12.))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::BOLD).text_color(p.text).child(if custom_dir { ov.screenshot_dir.clone() } else { "Resimler\\P2".into() }))
                    .when(!custom_dir, |r| r.child(hint("varsayılan", p))),
            )
            .child(
                row()
                    .child(button("shot-dir", "Klasör seç", Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| this.pick_shot_dir(window, cx))))
                    .when(custom_dir, |r| r.child(button("shot-default", "Varsayılan", Btn::Default, false, p).on_click(cx.listener(|this, _, _, cx| this.update_prefs(|_, o, _| o.screenshot_dir.clear(), cx))))),
            );

        div().flex().flex_col().child(location).child(download).child(game).child(keys).child(look).child(notes).child(shots).into_any_element()
    }

    pub(super) fn availability(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (_, _, av) = self.prefs(cx);
        if self.work_start.is_none() {
            let start = cx.new(|cx| InputState::new(window, cx).placeholder("--:--"));
            let end = cx.new(|cx| InputState::new(window, cx).placeholder("--:--"));
            let (s, e) = (av.work_start.clone(), av.work_end.clone());
            start.update(cx, |i, cx| i.set_value(s, window, cx));
            end.update(cx, |i, cx| i.set_value(e, window, cx));
            for (input, is_start) in [(start.clone(), true), (end.clone(), false)] {
                let sub = cx.subscribe(&input, move |this, input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. } | InputEvent::Change) {
                        let v = input.read(cx).value().trim().to_string();
                        let ok = v.len() == 5 && v.split_once(':').is_some_and(|(h, m)| h.parse::<u32>().is_ok_and(|h| h < 24) && m.parse::<u32>().is_ok_and(|m| m < 60));
                        let cur = this.prefs(cx).2;
                        if ok && (if is_start { cur.work_start != v } else { cur.work_end != v }) {
                            this.update_prefs(move |_, _, a| if is_start { a.work_start = v } else { a.work_end = v }, cx);
                        }
                    }
                });
                self.prefs_subs.push(sub);
            }
            self.work_start = Some(start);
            self.work_end = Some(end);
        }
        let mut days = div().flex().gap(px(5.));
        for (label, value) in [("Pz", 0u8), ("Pt", 1), ("Sa", 2), ("Ça", 3), ("Pe", 4), ("Cu", 5), ("Ct", 6)] {
            let on = av.work_days.contains(&value);
            days = days.child(
                div()
                    .id(SharedString::from(format!("day-{value}")))
                    .flex_1()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .rounded(radius::SM)
                    .cursor_pointer()
                    .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_strong) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text_2) })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.update_prefs(
                            move |_, _, a| {
                                if let Some(i) = a.work_days.iter().position(|d| *d == value) {
                                    a.work_days.remove(i);
                                } else {
                                    a.work_days.push(value);
                                    a.work_days.sort();
                                }
                            },
                            cx,
                        )
                    })),
            );
        }
        let time_field = |label: &'static str, input: &gpui_kit::Entity<InputState>, this: &Self, window: &Window, cx: &mut Context<Self>| {
            {
            // Chromium `input[type=time]` saat göstergesi (sağda 12px saat simgesi).
            const CLOCK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="6.4" fill="none" stroke="black" stroke-width="1.4"/><path d="M8 4.6V8l2.4 1.5" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round"/></svg>"#;
            let icon = gpui_kit::svg().data(CLOCK).size(px(10.)).mr(px(3.)).flex_shrink_0().text_color(p.text).into_any_element();
            div().flex_1().min_w_0().child(field(label, this.input_trailing(input, Some(icon), p, window, cx), p))
        }
        };
        let (ws, we) = (self.work_start.clone().unwrap(), self.work_end.clone().unwrap());
        let hours = div()
            .flex()
            .items_end()
            .gap(px(10.))
            .child(time_field("Başlangıç", &ws, self, window, cx))
            .child(div().pb(px(9.)).text_color(p.text_3).child("—"))
            .child(time_field("Bitiş", &we, self, window, cx));
        let mut outside = div().flex().flex_col().gap(px(6.));
        for (value, label, detail) in [("none", "Durumu değiştirme", "Seçtiğin durum korunur."), ("AWAY", "Uzakta göster", "Çalışma saatleri dışında otomatik olarak uzakta görün."), ("BUSY", "Meşgul göster", "Bildirimleri azaltmak istediğini ekibine göster.")] {
            outside = outside.child(choice(SharedString::from(format!("out-{value}")), label, detail, av.outside_hours_status == value, p).on_click(cx.listener(move |this, _, _, cx| this.update_prefs(move |_, _, a| a.outside_hours_status = value.into(), cx))));
        }
        div()
            .flex()
            .flex_col()
            .child(block("Çalışma günleri", p).child(hint("Ekip arkadaşların sana ne zaman ulaşabileceklerini bilir.", p)).child(days).child(hours))
            .child(last_block("Çalışma saatleri dışında", p).child(outside))
            .into_any_element()
    }
}

/// library.ts `formatBytes`.
fn fmt_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let index = (((bytes as f64).ln() / 1024f64.ln()).floor() as usize).min(UNITS.len() - 1);
    let v = bytes as f64 / 1024f64.powi(index as i32);
    if index == 0 { format!("{v:.0} {}", UNITS[0]) } else { format!("{v:.1} {}", UNITS[index]) }
}

#[allow(dead_code)]
fn _t() -> gpui_kit::Div {
    text(fs(1.))
}
