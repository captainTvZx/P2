//! Ayarlar > Görünüm (`src/AppearanceSettings.tsx`: 19 hazır tema ızgarası + özel tema, dışa/içe aktarma,
//! yazı tipi ve boyutu) ve Ayarlar > Dil ve Bölge (`LanguageSection`: dil ızgarası, saat dilimi, 24 saat,
//! haftanın ilk günü, önizleme). Tema seçimi `Palette::for_theme` / `palette_for` ile anında uygulanır.

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{Datelike, Timelike};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::AnimationExt as _;
use gpui_kit::{AnyElement, Context, FontWeight, SharedString, Window, div, px, relative};
use p2_client_core::store::{self, AppearancePrefs, CustomTheme, RegionPrefs, Settings};
use p2_ui::P2Text as _;
use p2_ui::controls::{TabsStyle, tab, tabs};
use p2_ui::palette::{ThemeMode, ThemeSpec, palette_for, themes};
use p2_ui::tokens::{Palette, css_rgba, font_size, fs, radius, shadows};

use super::settings_ui::{self, Btn, block, button, field, hint, last_block, row, select, select_menu, text, toggle_row};
use super::{SettingsModal, sliders};

/// theme.ts `FONTS` (kimlik, ad, DirectWrite aile adı). Görünen ad: [`font_label`] (`set2.font.<kimlik>`).
pub const FONTS: [(&str, &str, &str); 5] = [
    ("inter", "Inter (varsayılan)", p2_ui::tokens::FONT),
    ("system", "Sistem yazı tipi", "Segoe UI"),
    ("segoe", "Segoe UI", "Segoe UI"),
    ("georgia", "Georgia (tırnaklı)", "Georgia"),
    ("mono", "Tek aralıklı", "Cascadia Mono"),
];

static FONT_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Kök yazı ailesi (Ayarlar > Görünüm > Yazı tipi; `app.rs` kök öğesi kullanır).
pub fn font_family() -> &'static str {
    FONTS[FONT_INDEX.load(Ordering::Relaxed).min(FONTS.len() - 1)].2
}

/// Yazı tipinin görünen adı (çeviri).
pub fn font_label(id: &str) -> String {
    p2_i18n::t(&format!("set2.font.{id}"))
}

fn set_font(id: &str) {
    FONT_INDEX.store(FONTS.iter().position(|f| f.0 == id).unwrap_or(0), Ordering::Relaxed);
}

/// Özel temanın paleti (theme.ts `ThemeSpec`).
fn custom_spec(c: &CustomTheme) -> ThemeSpec {
    ThemeSpec {
        id: "custom",
        name: "custom",
        mode: if c.mode == "light" { ThemeMode::Light } else { ThemeMode::Dark },
        accent: c.accent.clone(),
        surface: c.surface.clone(),
        tint: c.tint,
        contrast: c.contrast,
        saturation: c.saturation,
    }
}

/// Ayarlardaki temadan palet (açılışta `main.rs`; özel tema dahil). Yazı tipini de kurar.
pub fn startup_palette(settings: &Settings) -> Palette {
    set_font(&settings.appearance.font);
    palette_of(settings.theme.as_deref().unwrap_or(""), &settings.appearance)
}

fn palette_of(theme: &str, a: &AppearancePrefs) -> Palette {
    if theme == "custom" {
        return palette_for(&custom_spec(&a.custom.clone().unwrap_or_default()));
    }
    Palette::for_theme(theme)
}

/// Kayıttaki tema → ızgarada seçili kimlik ("dark" → p2-mavi, "light" → kagit).
fn selected_id(theme: Option<&str>) -> String {
    match theme.unwrap_or("") {
        "" | "dark" => "p2-mavi".into(),
        "light" => "kagit".into(),
        other => other.into(),
    }
}

#[derive(Default)]
pub struct LookState {
    pub custom_tab: bool,
    pub tab_epoch: usize,
    /// Seçimi canlandırma sayacı (yeni seçilen kart halkayla belirir).
    pub pick_epoch: usize,
}

// ------------------------------------------------------------------------------ bölge biçimi

/// Sistem saat dilimi (IANA).
pub fn system_time_zone() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into())
}

/// i18n.ts `timeZones()` (eski motorlardaki kısa liste) — Intl verisi olmadığından bu liste.
pub const TIME_ZONES: [&str; 14] = ["Europe/Istanbul", "Europe/London", "Europe/Berlin", "Europe/Madrid", "America/New_York", "America/Chicago", "America/Los_Angeles", "America/Sao_Paulo", "Asia/Tokyo", "Asia/Seoul", "Asia/Shanghai", "Asia/Taipei", "Australia/Sydney", "UTC"];

/// Bölgenin şu anki UTC farkı (dakika): sabit fark + AB/ABD/Avustralya yaz saati kuralları.
fn zone_offset(zone: &str, now: chrono::DateTime<chrono::Utc>) -> i32 {
    let nth_sunday = |year: i32, month: u32, n: i32| -> chrono::NaiveDate {
        if n > 0 {
            let first = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap();
            let shift = (7 - first.weekday().num_days_from_sunday()) % 7;
            first + chrono::Duration::days(shift as i64 + 7 * (n as i64 - 1))
        } else {
            let next = if month == 12 { chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1) } else { chrono::NaiveDate::from_ymd_opt(year, month + 1, 1) }.unwrap();
            let last = next - chrono::Duration::days(1);
            last - chrono::Duration::days(last.weekday().num_days_from_sunday() as i64)
        }
    };
    let y = now.year();
    let d = now.date_naive();
    let eu = d >= nth_sunday(y, 3, -1) && d < nth_sunday(y, 10, -1);
    let us = d >= nth_sunday(y, 3, 2) && d < nth_sunday(y, 11, 1);
    let au = !(d >= nth_sunday(y, 4, 1) && d < nth_sunday(y, 10, 1));
    match zone {
        "Europe/Istanbul" => 180,
        "Europe/London" => if eu { 60 } else { 0 },
        "Europe/Berlin" | "Europe/Madrid" => if eu { 120 } else { 60 },
        "America/New_York" => if us { -240 } else { -300 },
        "America/Chicago" => if us { -300 } else { -360 },
        "America/Los_Angeles" => if us { -420 } else { -480 },
        "America/Sao_Paulo" => -180,
        "Asia/Tokyo" | "Asia/Seoul" => 540,
        "Asia/Shanghai" | "Asia/Taipei" => 480,
        "Australia/Sydney" => if au { 660 } else { 600 },
        _ => chrono::Local::now().offset().local_minus_utc() / 60,
    }
}

/// i18n.ts `zoneLabel`: "Europe/Istanbul (GMT+03:00)".
pub fn zone_label(zone: &str) -> String {
    let m = zone_offset(zone, chrono::Utc::now());
    let off = if m == 0 { "GMT".to_string() } else { format!("GMT{}{:02}:{:02}", if m < 0 { '-' } else { '+' }, m.abs() / 60, m.abs() % 60) };
    format!("{} ({off})", zone.replace('_', " "))
}

fn region() -> RegionPrefs {
    store::load_settings().region
}

/// Etkin bölge ayarına göre yerel zaman (saat dilimi otomatikse bilgisayarınki).
fn zoned(r: &RegionPrefs, at: chrono::DateTime<chrono::Utc>) -> chrono::NaiveDateTime {
    if r.auto_time_zone || r.time_zone.is_empty() {
        at.with_timezone(&chrono::Local).naive_local()
    } else {
        at.naive_utc() + chrono::Duration::minutes(zone_offset(&r.time_zone, at) as i64)
    }
}

fn clock(r: &RegionPrefs, t: chrono::NaiveDateTime) -> String {
    let lang = p2_i18n::language().id;
    if r.hour24 {
        return format!("{:02}:{:02}", t.hour(), t.minute());
    }
    let (pm, h) = t.hour12();
    match lang {
        "tr" => format!("{} {:02}:{:02}", if pm { "ÖS" } else { "ÖÖ" }, h, t.minute()),
        _ => format!("{:02}:{:02} {}", h, t.minute(), if pm { "PM" } else { "AM" }),
    }
}

/// i18n.ts `formatTime` (mesaj saati; 24 saat / ÖÖ-ÖS, saat dilimi ayarlı).
pub fn format_time(at: chrono::DateTime<chrono::Utc>) -> String {
    let r = region();
    clock(&r, zoned(&r, at))
}

fn format_date_time_with(r: &RegionPrefs, at: chrono::DateTime<chrono::Utc>) -> String {
    let t = zoned(r, at);
    let month = p2_i18n::month_name(t.month0() as usize);
    p2_i18n::tv("set2.dateTime", &[("day", &t.day().to_string()), ("month", &month), ("time", &clock(r, t))])
}

/// i18n.ts `formatDateTime` ("13 Kasım 06:14").
pub fn format_date_time(at: chrono::DateTime<chrono::Utc>) -> String {
    format_date_time_with(&region(), at)
}

// ------------------------------------------------------------------------------ tema önizlemesi

/// `.theme-preview`: 62px (özel temada 96) — ray (navy-900, 3 çubuk; ilki vurgu) + panel (navy-850;
/// başlık slate-100 %85, iki satır navy-600, vurgu düğmesi). Zemin navy-950, 1px gri kenarlık, köşe 8.
/// `padded`: kart içinde `.theme-card > span` kuralı önizlemeye de 0 3 2 3 iç boşluk verir.
fn preview(theme: &ThemeSpec, h: f32, padded: bool) -> gpui_kit::Div {
    let pal = palette_for(theme);
    let bar = |first: bool| div().w_full().h(px(8.)).rounded(px(3.)).map(|s| if first { s.bg(pal.accent_grad.background()) } else { s.bg(pal.navy_500) });
    div()
        .w_full()
        .h(px(h))
        .flex()
        .when(padded, |s| s.px(px(3.)).pb(px(2.)))
        .overflow_hidden()
        .rounded(radius::SM)
        .border_1()
        .border_color(css_rgba(127, 127, 127, 0.18))
        .bg(pal.navy_950)
        .child(div().w(px(22.)).h_full().flex_shrink_0().flex().flex_col().gap(px(4.)).px(px(4.)).py(px(5.)).bg(pal.navy_900).child(bar(true)).child(bar(false)).child(bar(false)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .gap(px(5.))
                .px(px(8.))
                .py(px(7.))
                .bg(pal.navy_850)
                .child(div().w(relative(0.48)).h(px(6.)).rounded(px(3.)).bg(pal.slate_100).opacity(0.85))
                .child(div().w(relative(0.8)).h(px(5.)).rounded(px(3.)).bg(pal.navy_600))
                .child(div().w(relative(0.55)).h(px(5.)).rounded(px(3.)).bg(pal.navy_600))
                .child(div().mt(px(2.)).w(px(30.)).h(px(9.)).rounded(px(3.)).bg(pal.accent_grad.background())),
        )
}

// ------------------------------------------------------------------------------ Windows renk seçici

/// `<input type="color">`: Chromium Windows'ta yerel renk iletişim kutusunu (ChooseColor) açar.
#[cfg(windows)]
fn choose_color(initial: &str) -> Option<String> {
    #[repr(C)]
    struct ChooseColorW {
        size: u32,
        owner: isize,
        instance: isize,
        rgb: u32,
        custom: *mut u32,
        flags: u32,
        cust_data: isize,
        hook: isize,
        template: *const u16,
    }
    #[link(name = "comdlg32")]
    unsafe extern "system" {
        fn ChooseColorW(cc: *mut ChooseColorW) -> i32;
    }
    let hex = initial.trim_start_matches('#');
    let v = u32::from_str_radix(hex, 16).unwrap_or(0x2c7bff);
    let (r, g, b) = ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
    let mut custom = [0x00ff_ffffu32; 16];
    let owner = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0 as isize;
    let mut cc = ChooseColorW { size: std::mem::size_of::<ChooseColorW>() as u32, owner, instance: 0, rgb: r | (g << 8) | (b << 16), custom: custom.as_mut_ptr(), flags: 0x1 | 0x2, cust_data: 0, hook: 0, template: std::ptr::null() };
    // CC_RGBINIT (1) | CC_FULLOPEN (2)
    if unsafe { ChooseColorW(&mut cc) } == 0 {
        return None;
    }
    let c = cc.rgb;
    Some(format!("#{:02x}{:02x}{:02x}", c & 0xff, (c >> 8) & 0xff, (c >> 16) & 0xff))
}

#[cfg(not(windows))]
fn choose_color(_: &str) -> Option<String> {
    None
}

impl SettingsModal {
    fn appearance_prefs(&self, cx: &Context<Self>) -> (String, AppearancePrefs) {
        self.app.upgrade().map(|a| {
            let s = &a.read(cx).settings;
            (selected_id(s.theme.as_deref()), s.appearance.clone())
        }).unwrap_or_else(|| ("p2-mavi".into(), AppearancePrefs::default()))
    }

    /// Temayı seç, kaydet ve bütün pencerelere uygula.
    pub(super) fn apply_theme(&mut self, theme: &str, cx: &mut Context<Self>) {
        let theme = theme.to_string();
        let mut palette = None;
        let _ = self.app.update(cx, |app, _| {
            app.settings.theme = Some(theme.clone());
            palette = Some(palette_of(&theme, &app.settings.appearance));
            store::save_settings(&app.settings);
        });
        if let Some(p) = palette {
            p2_ui::theme::apply(p, cx);
        }
        self.look.pick_epoch += 1;
        crate::trace(&format!("ayarlar: tema {theme}"));
        cx.refresh_windows();
    }

    fn update_appearance(&mut self, change: impl FnOnce(&mut AppearancePrefs), cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, _| {
            change(&mut app.settings.appearance);
            store::save_settings(&app.settings);
            // Yazı boyutu canlı uygulanır (Electron applyAppearance gibi).
            p2_ui::fx::set_font_scale_percent(app.settings.appearance.font_scale as f32);
        });
        cx.refresh_windows();
        cx.notify();
    }

    /// Özel temayı değiştir (theme.ts `patchCustom`: tema `custom` olur) ve uygula.
    fn patch_custom(&mut self, change: impl FnOnce(&mut CustomTheme), cx: &mut Context<Self>) {
        self.update_appearance(
            |a| {
                let mut c = a.custom.clone().unwrap_or_default();
                change(&mut c);
                a.custom = Some(c);
            },
            cx,
        );
        self.apply_theme("custom", cx);
    }

    fn export_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (id, a) = self.appearance_prefs(cx);
        let (name, json) = if id == "custom" {
            let c = a.custom.clone().unwrap_or_default();
            let v = serde_json::json!({"p2Theme": 2, "id": "custom", "name": c.name, "mode": c.mode, "accent": c.accent, "surface": c.surface, "tint": c.tint, "contrast": c.contrast, "saturation": c.saturation});
            (c.name, v)
        } else {
            let t = themes().into_iter().find(|t| t.id == id).unwrap_or_else(|| themes().remove(0));
            let v = serde_json::json!({"p2Theme": 2, "id": t.id, "name": t.name, "mode": if t.mode == ThemeMode::Light { "light" } else { "dark" }, "accent": t.accent, "surface": t.surface, "tint": t.tint, "contrast": t.contrast, "saturation": t.saturation});
            (t.name.to_string(), v)
        };
        let file = format!("{}.p2theme.json", settings_ui::lower_tr(&name).split_whitespace().collect::<Vec<_>>().join("-"));
        let dir = dirs_downloads();
        let rx = cx.prompt_for_new_path(&dir, Some(&file));
        let body = serde_json::to_string_pretty(&json).unwrap_or_default();
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let ok = std::fs::write(&path, body).is_ok();
                let _ = this.update(cx, |_, cx| {
                    if ok {
                        crate::toasts::ok(p2_i18n::t("look.exported"));
                    } else {
                        crate::toasts::error(p2_i18n::t("toast.failed"));
                    }
                    crate::trace(&format!("ayarlar: tema dışa aktarıldı {}", path.display()));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn import_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: None });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.into_iter().next() {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    let _ = this.update(cx, |this, cx| this.import_theme_text(&text, cx));
                }
            }
        })
        .detach();
    }

    /// AppearanceSettings.tsx `importTheme`.
    pub(super) fn import_theme_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let data: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
        let s = |k: &str| data.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let (Some(accent), Some(surface)) = (s("accent"), s("surface")) else {
            crate::toasts::warn(p2_i18n::t("look.notTheme"));
            cx.notify();
            return;
        };
        let num = |k: &str, d: f32| data.get(k).and_then(|v| v.as_f64()).map(|v| v as f32).filter(|v| *v != 0.).unwrap_or(d);
        let name: String = s("name").unwrap_or_else(|| p2_i18n::t("set2.importedTheme")).chars().take(40).collect();
        let mode = if s("mode").as_deref() == Some("light") { "light" } else { "dark" };
        let (tint, contrast, saturation) = (num("tint", 0.5), num("contrast", 1.), num("saturation", 1.));
        self.patch_custom(move |c| *c = CustomTheme { name, mode: mode.into(), accent, surface, tint, contrast, saturation }, cx);
        self.look.custom_tab = true;
        self.custom_name = None;
        crate::toasts::ok(p2_i18n::t("look.imported"));
        cx.notify();
    }

    pub(super) fn appearance(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (current, a) = self.appearance_prefs(cx);
        let custom = a.custom.clone().unwrap_or_default();
        let tab_on = self.look.custom_tab;
        let te = self.look.tab_epoch;
        let tab_row = div().flex().child(
            tabs(TabsStyle::Segment, p)
                .child(tab("look-tab-p2", p2_i18n::t("look.p2themes"), !tab_on, TabsStyle::Segment, te, p).on_click(cx.listener(|this, _, _, cx| {
                    this.look.custom_tab = false;
                    this.look.tab_epoch += 1;
                    cx.notify();
                })))
                .child(tab("look-tab-custom", p2_i18n::t("look.custom"), tab_on, TabsStyle::Segment, te, p).on_click(cx.listener(|this, _, _, cx| {
                    this.look.custom_tab = true;
                    this.look.tab_epoch += 1;
                    cx.notify();
                }))),
        );
        let mut theme_block = block(&p2_i18n::t("look.theme"), p).child(tab_row);
        if !tab_on {
            // Izgara: repeat(auto-fill, minmax(118px, 1fr)), gap 8.
            let width = self.content_width(window);
            let cols = (((width + 8.) / 126.).floor() as usize).max(1);
            let card_w = (width - 8. * (cols as f32 - 1.)) / cols as f32;
            let all = themes();

            let pick_epoch = self.look.pick_epoch;
            let grid = |list: Vec<ThemeSpec>, cx: &mut Context<Self>| {
                let mut g = div().flex().flex_wrap().gap(px(8.));
                for t in list {
                    let on = t.id == current;
                    let id = t.id;
                    let (hb, hbd) = (p.bg_3, p.border_strong);
                    let card = div()
                            .id(SharedString::from(format!("theme-{id}")))
                            .w(px(card_w))
                            .flex()
                            .flex_col()
                            .gap(px(7.))
                            .p(px(6.))
                            .border_1()
                            .rounded(radius::MD)
                            .bg(p.bg_2)
                            .cursor_pointer()
                            .map(|s| {
                                if on {
                                    s.border_color(p.accent_hover).shadow(shadows(&[p.ring]))
                                } else {
                                    s.border_color(p.border_subtle).hover(move |s| s.border_color(hbd).bg(hb).mt(px(-1.)).mb(px(1.)))
                                }
                            })
                            .on_click(cx.listener(move |this, _, _, cx| this.apply_theme(id, cx)))
                            .child(preview(&t, 62., true))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.))
                                    .px(px(3.))
                                    .pb(px(2.))
                                    .child(text(font_size::SM).css_weight(650.).text_color(p.text_strong).child(p2_i18n::t(&format!("theme.{}", t.id))))
                                    .child(text(font_size::XXS).text_color(p.text_3).child(p2_i18n::t(if t.mode == ThemeMode::Light { "look.light" } else { "look.dark" }))),
                            );
                    // Seçim: kenarlık ve odak halkası 140ms içinde belirir (CSS transition border-color, box-shadow --m-fast).
                    let card: AnyElement = if on && pick_epoch > 0 {
                        let (ring, from, to) = (p.ring, p.border_subtle, p.accent_hover);
                        card.with_animation(("theme-pick", pick_epoch), gpui_kit::Animation::new(p2_ui::motion::M_FAST), move |el, t| {
                            let mut r = ring;
                            r.color.a *= t;
                            el.border_color(mix(from, to, t)).shadow(shadows(&[r]))
                        })
                        .into_any_element()
                    } else {
                        card.into_any_element()
                    };
                    g = g.child(card);
                }
                g
            };
            let dark: Vec<_> = all.iter().filter(|t| t.mode == ThemeMode::Dark).cloned().collect();
            let light: Vec<_> = all.iter().filter(|t| t.mode == ThemeMode::Light).cloned().collect();
            theme_block = theme_block
                .child(grid(dark, cx))
                .child(div().mt(px(6.)).child(settings_ui::heading(&p2_i18n::t("look.light"), p)))
                .child(grid(light, cx));
        } else {
            if self.custom_name.is_none() {
                let input = cx.new(|cx| InputState::new(window, cx));
                let n = custom.name.clone();
                input.update(cx, |s, cx| s.set_value(n, window, cx));
                let sub = cx.subscribe(&input, |this, input, e: &InputEvent, cx| {
                    if matches!(e, InputEvent::Change) {
                        let v: String = input.read(cx).value().chars().take(40).collect();
                        this.patch_custom(move |c| c.name = v, cx);
                    }
                });
                self.prefs_subs.push(sub);
                self.custom_name = Some(input);
            }
            let name_input = self.custom_name.clone().map(|i| self.input(&i, p, window, cx));
            let dark = custom.mode != "light";
            let mode_tabs = div().flex().child(
                tabs(TabsStyle::Segment, p)
                    .child(tab("look-mode-dark", p2_i18n::t("look.dark"), dark, TabsStyle::Segment, te, p).on_click(cx.listener(|this, _, _, cx| {
                        this.patch_custom(|c| {
                            if c.mode == "light" {
                                c.surface = CustomTheme::default().surface;
                            }
                            c.mode = "dark".into();
                        }, cx)
                    })))
                    .child(tab("look-mode-light", p2_i18n::t("look.light"), !dark, TabsStyle::Segment, te, p).on_click(cx.listener(|this, _, _, cx| {
                        this.patch_custom(|c| {
                            if c.mode == "dark" {
                                c.surface = "#dfe4ec".into();
                            }
                            c.mode = "light".into();
                        }, cx)
                    }))),
            );
            let swatch = |id: &'static str, value: &str, accent: bool, cx: &mut Context<Self>| {
                let init = value.to_string();
                let color = gpui_kit::rgb(u32::from_str_radix(value.trim_start_matches('#'), 16).unwrap_or(0));
                div()
                    .id(id)
                    .w(px(54.))
                    .h(px(30.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    // Chromium ::-webkit-color-swatch-wrapper (p 4 2) + ::-webkit-color-swatch (1px #777).
                    .child(div().w(px(44.)).h(px(16.)).border_1().border_color(gpui_kit::rgb(0x777777)).bg(color))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(c) = choose_color(&init) {
                            this.patch_custom(move |t| if accent { t.accent = c } else { t.surface = c }, cx);
                        }
                    }))
            };
            let tint = (custom.tint * 100.).round() as i32;
            let contrast = (custom.contrast * 100.).round() as i32;
            let saturation = (custom.saturation * 100.).round() as i32;
            let rng = |id: &str, v: i32, min: i32, max: i32, apply: fn(&mut CustomTheme, f32), this: &Self, cx: &mut Context<Self>| {
                settings_ui::range(id, v, min, max, 1, true, p, sliders, &this.sliders, std::rc::Rc::new(move |this: &mut Self, v, cx| this.patch_custom(move |c| apply(c, v as f32 / 100.), cx)), cx)
            };
            let fields = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(9.))
                .child(field(p2_i18n::t("look.themeName"), div().children(name_input), p))
                .child(mode_tabs)
                .child(field(p2_i18n::t("look.accent"), swatch("look-accent", &custom.accent, true, cx), p))
                .child(field(p2_i18n::t("look.surface"), swatch("look-surface", &custom.surface, false, cx), p))
                .child(field(p2_i18n::tv("look.tint", &[("value", &tint.to_string())]), rng("look-tint", tint, 0, 100, |c, v| c.tint = v, self, cx), p))
                .child(field(p2_i18n::tv("look.contrast", &[("value", &contrast.to_string())]), rng("look-contrast", contrast, 80, 125, |c, v| c.contrast = v, self, cx), p))
                .child(field(p2_i18n::tv("look.saturation", &[("value", &saturation.to_string())]), rng("look-saturation", saturation, 40, 140, |c, v| c.saturation = v, self, cx), p));
            theme_block = theme_block.child(div().flex().items_start().gap(px(14.)).child(div().w(px(150.)).flex_shrink_0().child(preview(&custom_spec(&custom), 96., false))).child(fields));
        }
        theme_block = theme_block
            .child(
                row()
                    .child(button("look-export", p2_i18n::t("look.export"), Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| this.export_theme(window, cx))))
                    .child(button("look-import", p2_i18n::t("look.import"), Btn::Default, false, p).on_click(cx.listener(|this, _, window, cx| this.import_theme(window, cx)))),
            );

        // Yazı tipi
        let font_ix = FONTS.iter().position(|f| f.0 == a.font).unwrap_or(0);
        let open = self.open_select == Some("font");
        let font_sel = div()
            .child(select("sel-font", font_label(FONTS[font_ix].0), open, p).on_click(cx.listener(|this, _, _, cx| this.toggle_select("font", cx))))
            .when(open, |s| {
                s.child(select_menu("font", &FONTS.iter().map(|f| SharedString::from(font_label(f.0))).collect::<Vec<_>>(), font_ix, self.content_w, p, cx, |this: &mut Self, i, _, cx| {
                    this.open_select = None;
                    let id = FONTS[i].0;
                    set_font(id);
                    this.update_appearance(|a| a.font = id.into(), cx);
                    crate::trace(&format!("ayarlar: yazı tipi {id}"));
                    cx.refresh_windows();
                }))
            });
        let scale = a.font_scale as i32;
        let scale_range = settings_ui::range("look-scale", scale, 70, 125, 5, true, p, sliders, &self.sliders, std::rc::Rc::new(|this: &mut Self, v, cx| {
            this.update_appearance(|a| a.font_scale = v as u32, cx);
            crate::trace(&format!("ayarlar: yazı boyutu {v}"));
        }), cx);
        let font_block = last_block(&p2_i18n::t("look.font"), p)
            .child(field(p2_i18n::t("look.font"), font_sel, p))
            .child(field(p2_i18n::tv("look.fontSize", &[("value", &scale.to_string())]), scale_range, p))
            .child(hint(p2_i18n::t("look.sample"), p).font_family(FONTS[font_ix].2));

        div().flex().flex_col().child(theme_block).child(font_block).into_any_element()
    }

    /// Kaydırma alanının içerik genişliği (598 / taşma yoksa 608).
    pub(super) fn content_width(&self, window: &Window) -> f32 {
        let vp = window.viewport_size();
        let modal_w = (f32::from(vp.width) - 48.).min(880.);
        modal_w - 2. - 230. - 40. - if self.overflow { 10. } else { 0. }
    }

    pub(super) fn toggle_select(&mut self, name: &'static str, cx: &mut Context<Self>) {
        self.open_select = if self.open_select == Some(name) { None } else { Some(name) };
        cx.notify();
    }

    // ------------------------------------------------------------------ Dil ve Bölge

    fn set_language(&mut self, id: &'static str, cx: &mut Context<Self>) {
        p2_i18n::set_language(id);
        let _ = self.app.update(cx, |app, _| {
            app.settings.language = Some(id.into());
            store::save_settings(&app.settings);
        });
        crate::trace(&format!("ayarlar: dil {id}"));
        cx.refresh_windows();
    }

    fn update_region(&mut self, change: impl FnOnce(&mut RegionPrefs), cx: &mut Context<Self>) {
        let _ = self.app.update(cx, |app, _| {
            change(&mut app.settings.region);
            store::save_settings(&app.settings);
            let r = &app.settings.region;
            crate::trace(&format!("ayarlar: bölge otomatik={} dilim={} 24saat={} hafta={}", r.auto_time_zone, r.time_zone, r.hour24, r.week_start));
        });
        cx.notify();
    }

    pub(super) fn language(&mut self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let half = (self.content_width(window) - 6.) / 2.;
        let current = p2_i18n::language().id;
        let mut grid = div().flex().flex_wrap().gap(px(6.));
        for lang in p2_i18n::LANGUAGES {
            let on = lang.id == current;
            let id = lang.id;
            let (hb, ht) = (p.bg_hover, p.text_strong);
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("lang-{id}")))
                    .w(px(half))
                    .min_h(px(43.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .px(px(10.))
                    .py(px(7.))
                    .border_1()
                    .rounded(radius::SM)
                    .cursor_pointer()
                    .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_strong) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hb).text_color(ht)) })
                    .on_click(cx.listener(move |this, _, _, cx| this.set_language(id, cx)))
                    // CJK adlar yedek yazı tipinin satır yüksekliğiyle (Chromium: 18px).
                    .child(text(font_size::ROOT).font_weight(FontWeight::BOLD).when(lang.native.chars().any(|c| c as u32 >= 0x3000), |s| s.line_height(px(18.))).child(lang.native))
                    .when(lang.native != lang.english, |s| s.child(text(fs(9.)).text_color(p.text_3).child(lang.english))),
            );
        }
        let r = self.app.upgrade().map(|a| a.read(cx).settings.region.clone()).unwrap_or_default();
        let zone_sys = system_time_zone();
        let mut region = last_block(&p2_i18n::t("lang.region"), p).child(
            toggle_row("reg-auto", p2_i18n::t("lang.autoZone"), Some(p2_i18n::tv("lang.autoZone.hint", &[("zone", &zone_sys)]).into()), r.auto_time_zone, self.epoch("reg-auto"), p).on_click(cx.listener(|this, _, _, cx| {
                this.bump("reg-auto");
                this.update_region(|r| r.auto_time_zone = !r.auto_time_zone, cx)
            })),
        );
        if !r.auto_time_zone {
            let cur = if r.time_zone.is_empty() { zone_sys.clone() } else { r.time_zone.clone() };
            let mut zones: Vec<String> = TIME_ZONES.iter().map(|z| z.to_string()).collect();
            if !zones.contains(&cur) {
                zones.insert(0, cur.clone());
            }
            let ix = zones.iter().position(|z| *z == cur).unwrap_or(0);
            let open = self.open_select == Some("zone");
            let labels: Vec<SharedString> = zones.iter().map(|z| SharedString::from(zone_label(z))).collect();
            let zones2 = zones.clone();
            region = region.child(field(
                p2_i18n::t("lang.zone"),
                div().child(select("sel-zone", labels[ix].clone(), open, p).on_click(cx.listener(|this, _, _, cx| this.toggle_select("zone", cx)))).when(open, |s| {
                    s.child(select_menu("zone", &labels, ix, self.content_w, p, cx, move |this: &mut Self, i, _, cx| {
                        this.open_select = None;
                        let z = zones2[i].clone();
                        this.update_region(move |r| r.time_zone = z, cx);
                    }))
                }),
                p,
            ));
        }
        let weeks = [("auto", "lang.weekStart.auto"), ("monday", "lang.weekStart.monday"), ("sunday", "lang.weekStart.sunday")];
        let wix = weeks.iter().position(|w| w.0 == r.week_start).unwrap_or(0);
        let wopen = self.open_select == Some("week");
        let wlabels: Vec<SharedString> = weeks.iter().map(|w| SharedString::from(p2_i18n::t(w.1))).collect();
        let now = chrono::Utc::now();
        let card = |label: String, value: String| div().flex_1().flex().flex_col().gap(px(3.)).p(px(10.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_3).child(text(fs(9.)).text_color(p.text_3).child(label)).child(text(fs(11.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(value));
        let t = zoned(&r, now);
        region = region
            .child(toggle_row("reg-24", p2_i18n::t("lang.hour24"), Some(p2_i18n::t("lang.hour24.hint").into()), r.hour24, self.epoch("reg-24"), p).on_click(cx.listener(|this, _, _, cx| {
                this.bump("reg-24");
                this.update_region(|r| r.hour24 = !r.hour24, cx)
            })))
            .child(field(
                p2_i18n::t("lang.weekStart"),
                div().child(select("sel-week", wlabels[wix].clone(), wopen, p).on_click(cx.listener(|this, _, _, cx| this.toggle_select("week", cx)))).when(wopen, |s| {
                    s.child(select_menu("week", &wlabels, wix, self.content_w, p, cx, move |this: &mut Self, i, _, cx| {
                        this.open_select = None;
                        let w = weeks[i].0;
                        this.update_region(move |r| r.week_start = w.into(), cx);
                    }))
                }),
                p,
            ))
            .child(div().flex().gap(px(7.)).child(card(p2_i18n::t("lang.previewTime"), clock(&r, t))).child(card(p2_i18n::t("lang.previewFull"), format_date_time_with(&r, now))));

        div()
            .flex()
            .flex_col()
            .child(block(&p2_i18n::t("lang.language"), p).child(hint(p2_i18n::t("lang.languageHint"), p)).child(grid))
            .child(region)
            .into_any_element()
    }
}

fn dirs_downloads() -> std::path::PathBuf {
    std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join("Downloads")).unwrap_or_else(std::env::temp_dir)
}

/// İki renk arası doğrusal karışım (CSS geçişi).
fn mix(a: gpui_kit::Hsla, b: gpui_kit::Hsla, t: f32) -> gpui_kit::Hsla {
    let (x, y): (gpui_kit::Rgba, gpui_kit::Rgba) = (a.into(), b.into());
    gpui_kit::Rgba { r: x.r + (y.r - x.r) * t, g: x.g + (y.g - x.g) * t, b: x.b + (y.b - x.b) * t, a: x.a + (y.a - x.a) * t }.into()
}
