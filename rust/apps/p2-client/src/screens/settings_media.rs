//! Ayarlar → Ses ve Görüntü (SettingsModal.tsx `MediaSection` + `VoiceSection`): giriş/çıkış aygıtı ve
//! seviyeleri, mikrofon testi (seviye çubuğu), kamera, ses işleme, ses kalitesi, konuşma modu ve tuş ataması.
//! Ayarlar profilin `settings.json`'una yazılır ve açık arama motoruna hemen uygulanır. Testlerde
//! (`P2_NO_AUDIO_DEVICE`) hiçbir ses aygıtı açılmaz ya da listelenmez; listede yalnız "Sistem varsayılanı" kalır.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, SharedString, Window, div, px, relative};
use p2_client_core::store::{self, MediaSettings};
use p2_media::DeviceInfo;
use p2_ui::P2Text as _;
use p2_ui::tokens::{Palette, css_rgba, fs, radius};

use super::settings_ui::{Btn, block, button, choice, hint, last_block, select, select_menu, slider, toggle_row};
use super::{SettingsModal, sliders};
use crate::task;

/// Aygıt listesi (arka planda okunur: WASAPI listesi birkaç yüz ms sürebilir).
#[derive(Default)]
pub struct MediaState {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
    pub cameras: Vec<p2_media::CameraInfo>,
    pub loaded: bool,
    pub capturing_key: bool,
    pub camera_on: bool,
}

/// Bas-konuş tuşunun okunur adı (SettingsModal `keyLabel`: Space → "Boşluk").
pub fn key_label(key: &str) -> String {
    match key {
        "space" => "Boşluk".into(),
        k => k.to_uppercase(),
    }
}

/// SettingsModal.tsx `voiceModes`.
const VOICE_MODES: [(&str, &str, &str); 3] = [
    ("open", "Normal (açık mikrofon)", "Mikrofonun sürekli açık; sustur düğmesiyle kapatırsın."),
    ("ptt", "Bas-konuş", "Yalnızca tuşa basılı tuttuğun sürece sesin gider."),
    ("ptm", "Bas-sustur", "Mikrofon açık; tuşa bastığın sürece susarsın."),
];

const BITRATES: [(u32, &str); 4] = [(32, "Ekonomik · 32 kbps"), (64, "Dengeli · 64 kbps"), (96, "Yüksek · 96 kbps (önerilen)"), (128, "En yüksek · 128 kbps")];

impl SettingsModal {
    pub(super) fn load_devices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.media.loaded {
            return;
        }
        self.media.loaded = true;
        if std::env::var_os("P2_NO_AUDIO_DEVICE").is_some() {
            return;
        }
        task::run(window, cx, || (p2_media::input_devices(), p2_media::output_devices(), p2_media::cameras()), |this, (inputs, outputs, cameras), _, cx| {
            crate::trace(&format!("ayarlar: {} giriş, {} çıkış aygıtı, {} kamera", inputs.len(), outputs.len(), cameras.len()));
            this.media.inputs = inputs;
            this.media.outputs = outputs;
            this.media.cameras = cameras;
            cx.notify();
        });
    }

    /// Ayarı değiştir, kaydet ve açık arama motoruna uygula.
    pub(super) fn update_media(&mut self, change: impl FnOnce(&mut MediaSettings), cx: &mut Context<Self>) {
        let Some(mut media) = self.app.upgrade().map(|app| app.read(cx).settings.media.clone()) else { return };
        change(&mut media);
        let saved = media.clone();
        let _ = self.app.update(cx, |app, _| {
            app.settings.media = saved;
            store::save_settings(&app.settings);
        });
        self.calls.update(cx, |c, cx| c.apply_settings(media, cx));
        cx.notify();
    }

    /// Bas-konuş tuşu yakalanırken basılan tuş (Esc vazgeçer).
    pub(super) fn captured_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if !self.media.capturing_key {
            return false;
        }
        self.media.capturing_key = false;
        if key != "escape" {
            let key = key.to_string();
            crate::trace(&format!("ayarlar: bas-konuş tuşu {key}"));
            self.update_media(move |m| m.voice_key = key, cx);
        }
        cx.notify();
        true
    }

    /// Seçim kutusu + açıkken liste. `options[0]` "Sistem varsayılanı".
    #[allow(clippy::too_many_arguments)]
    fn media_select(&self, name: &'static str, options: Vec<SharedString>, selected: usize, p: &Palette, cx: &mut Context<Self>, pick: impl Fn(&mut Self, usize, &mut Context<Self>) + 'static) -> gpui_kit::Div {
        let open = self.open_select == Some(name);
        div()
            .child(select(SharedString::from(format!("sel-{name}")), options[selected.min(options.len() - 1)].clone(), open, p).on_click(cx.listener(move |this, _, _, cx| this.toggle_select(name, cx))))
            .when(open, |s| {
                s.child(select_menu(name, &options, selected, self.content_w, p, cx, move |this: &mut Self, i, _, cx| {
                    this.open_select = None;
                    pick(this, i, cx);
                }))
            })
    }

    pub(super) fn media(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.load_devices(window, cx);
        let m = self.app.upgrade().map(|app| app.read(cx).settings.media.clone()).unwrap_or_default();
        let calls = self.calls.read(cx);
        let testing = calls.mic_test;
        // -60…0 dBFS → %0…100 (Electron: tepe × 140 × giriş seviyesi).
        let level = if testing { (((calls.mic_level + 60.0) / 60.0).clamp(0.0, 1.0) * m.input_volume as f32 / 100.).min(1.) } else { 0. };
        let def = SharedString::from("Sistem varsayılanı");

        let names = |list: &[DeviceInfo]| std::iter::once(def.clone()).chain(list.iter().map(|d| SharedString::from(d.name.clone()))).collect::<Vec<_>>();
        let pos = |list: &[DeviceInfo], cur: &Option<String>| cur.as_ref().and_then(|c| list.iter().position(|d| &d.name == c)).map_or(0, |i| i + 1);
        let in_sel = self.media_select("in-dev", names(&self.media.inputs), pos(&self.media.inputs, &m.input_device), p, cx, |this, i, cx| {
            let name = (i > 0).then(|| this.media.inputs[i - 1].name.clone());
            this.update_media(move |m| m.input_device = name, cx);
        });
        let out_sel = self.media_select("out-dev", names(&self.media.outputs), pos(&self.media.outputs, &m.output_device), p, cx, |this, i, cx| {
            let name = (i > 0).then(|| this.media.outputs[i - 1].name.clone());
            this.update_media(move |m| m.output_device = name, cx);
        });
        let cam_names = std::iter::once(def.clone()).chain(self.media.cameras.iter().map(|c| SharedString::from(c.name.clone()))).collect::<Vec<_>>();
        let cam_ix = m.camera_device.as_ref().and_then(|c| self.media.cameras.iter().position(|d| &d.id == c)).map_or(0, |i| i + 1);
        let cam_sel = self.media_select("cam", cam_names, cam_ix, p, cx, |this, i, cx| {
            let id = (i > 0).then(|| this.media.cameras[i - 1].id.clone());
            this.update_media(move |m| m.camera_device = id, cx);
        });
        let br_ix = BITRATES.iter().position(|b| b.0 == m.voice_bitrate).unwrap_or(2);
        let br_sel = self.media_select("bitrate", BITRATES.iter().map(|b| SharedString::from(b.1)).collect(), br_ix, p, cx, |this, i, cx| {
            let kbps = BITRATES[i].0;
            this.update_media(move |m| m.voice_bitrate = kbps, cx);
        });

        let vin = (m.input_volume.min(100)) as i32;
        let vout = (m.output_volume.min(100)) as i32;
        let in_slider = slider("vin", "Giriş ses seviyesi", vin, format!("{vin}%"), 0, 100, 1, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| this.update_media(move |m| m.input_volume = v as u32, cx)), cx);
        let out_slider = slider("vout", "Çıkış ses seviyesi", vout, format!("{vout}%"), 0, 100, 1, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| this.update_media(move |m| m.output_volume = v as u32, cx)), cx);

        // `.mic-test`: düğme (h 32, p 0 13) + seviye çubuğu (h 8, r 5, yeşil → sarı %72 → kırmızı).
        let (hb, hbd) = (p.bg_hover, p.border_strong);
        let mic_btn = div()
            .id("mic-test")
            .h(px(32.))
            .px(px(13.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .border_1()
            .rounded(radius::SM)
            .text_size(fs(11.))
            .css_weight(650.)
            .cursor_pointer()
            .map(|s| if testing { s.border_color(p.border_accent).bg(p.accent_soft_2).text_color(p.text_strong) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text).hover(move |s| s.bg(hb).border_color(hbd)) })
            .child(if testing { "Testi durdur" } else { "Mikrofonu test et" })
            .on_click(cx.listener(|this, _, window, cx| {
                let on = !this.calls.read(cx).mic_test;
                crate::trace(&format!("ayarlar: mikrofon testi {}", if on { "açık" } else { "kapalı" }));
                this.calls.update(cx, |c, cx| c.set_mic_test(on, false, window, cx));
            }));
        let meter = div().flex_1().h(px(8.)).rounded(px(5.)).bg(p.navy_600).overflow_hidden().child(
            div()
                .h_full()
                .w(relative(level))
                .rounded(px(5.))
                .overflow_hidden()
                .flex()
                // linear-gradient(90deg, success, warning 72%, danger): GPUI 2 durak → iki parça.
                .child(div().h_full().w(relative(0.72)).bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(p.success, 0.), gpui_kit::linear_color_stop(p.warning, 1.))))
                .child(div().h_full().flex_1().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(p.warning, 0.), gpui_kit::linear_color_stop(p.danger, 1.)))),
        );

        // `.camera-preview`: 16/9, en çok 200px yükseklik, r 11, bg-0; kapalıyken ortada "Önizleme kapalı".
        let preview = div()
            .w(px(200. * 16. / 9.))
            .h(px(200.))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.bg_0)
            .child(div().text_size(fs(11.)).text_color(p.text_4).child(if self.media.camera_on { "Kamera önizlemesi" } else { "Önizleme kapalı" }));
        let cam_on = self.media.camera_on;

        let mut voice = div().flex().flex_col().gap(px(6.));
        for (value, label, detail) in VOICE_MODES {
            voice = voice.child(choice(SharedString::from(format!("mode-{value}")), label, detail, m.voice_mode == value, p).on_click(cx.listener(move |this, _, _, cx| {
                crate::trace(&format!("ayarlar: konuşma modu {value}"));
                this.update_media(|m| m.voice_mode = value.into(), cx)
            })));
        }
        let open_mic = m.voice_mode == "open";
        let capturing = self.media.capturing_key;
        let bind = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .min_w(px(120.))
                    .px(px(12.))
                    .pt(px(7.))
                    .pb(px(6.))
                    .flex()
                    .justify_center()
                    .border_1()
                    .border_b_2()
                    .border_color(css_rgba(140, 190, 255, 0.42))
                    .rounded(radius::SM)
                    .bg(gpui_kit::linear_gradient(180., gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.14), 0.), gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.06), 1.)))
                    .text_color(p.white)
                    .font_family(p2_ui::tokens::FONT_MONO)
                    .text_size(px(11.))
                    .css_weight(650.)
                    .child(if capturing { "Bir tuşa bas…".to_string() } else { key_label(&m.voice_key) }),
            )
            .child(button("ptt-key", if capturing { "Bekleniyor" } else { "Tuşu değiştir" }, Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| {
                this.media.capturing_key = true;
                window.focus(&this.focus, cx);
                cx.notify();
            })));

        let toggles: [(&'static str, &str, &str, bool, fn(&mut MediaSettings)); 3] = [
            ("ec", "Yankı engelleme", "Hoparlörden geleni mikrofondan ayıklar", m.echo_cancellation, |m| m.echo_cancellation = !m.echo_cancellation),
            ("ns", "Gürültü bastırma", "Fan, klavye gibi arka plan seslerini azaltır", m.noise_suppression, |m| m.noise_suppression = !m.noise_suppression),
            ("agc", "Otomatik kazanç", "Ses seviyeni otomatik dengeler", m.auto_gain, |m| m.auto_gain = !m.auto_gain),
        ];
        let mut processing = block("Ses işleme", p);
        for (id, label, detail, on, flip) in toggles {
            processing = processing.child(toggle_row(id, label, Some(detail.into()), on, self.epoch(id), p).on_click(cx.listener(move |this, _, _, cx| {
                this.bump(id);
                this.update_media(flip, cx)
            })));
        }

        div()
            .flex()
            .flex_col()
            .child(block("Giriş cihazı", p).child(in_sel).child(in_slider).child(div().flex().items_center().gap(px(11.)).child(mic_btn).child(meter)).child(hint("Konuş; çubuk hareket ediyorsa mikrofon çalışıyor demektir.", p)))
            .child(block("Çıkış cihazı", p).child(out_sel).child(out_slider))
            .child(
                block("Kamera", p).child(cam_sel).child(preview).child(div().flex().child(button("cam-test", if cam_on { "Önizlemeyi kapat" } else { "Kamerayı test et" }, Btn::Default, false, p).on_click(cx.listener(|this, _, _, cx| {
                    this.media.camera_on = !this.media.camera_on;
                    crate::trace(&format!("ayarlar: kamera önizlemesi {}", if this.media.camera_on { "açık" } else { "kapalı" }));
                    cx.notify();
                })))),
            )
            .child(processing)
            .child(block("Ses kalitesi", p).child(br_sel).child(hint("Opus kodeğinin hedef bit hızı. Yüksek değer daha net ses verir, biraz daha fazla bant genişliği kullanır; sonraki bağlantıda devreye girer.", p)))
            .child(if open_mic { last_block("Konuşma modu", p).child(voice) } else { block("Konuşma modu", p).child(voice) })
            .when(!open_mic, |s| s.child(last_block("Tuş ataması", p).child(bind).child(hint("Yazı yazarken bu tuş metin girişini bozmaz; yalnızca sohbet dışında dinlenir. Vazgeçmek için Esc.", p))))
            .into_any_element()
    }
}
