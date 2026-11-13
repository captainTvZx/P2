# Electron → Rust token eşlemesi (P2 Mavi)

Electron referanstır. Rust tarafı `rust/crates/p2-ui` içindedir. Bir değer burada yoksa önce çalışan
Electron'dan **hesaplanmış** değeri alın (`dom-styles.mjs`), CSS dosyasını tahmin etmeyin.

| Kaynak (Electron) | Rust |
|---|---|
| `src/p2-theme.css` `:root` (P2 Mavi sabitleri) | `p2_ui::tokens::Palette::p2_dark()` |
| `src/theme.ts` `P2_THEMES` + `paletteFor()` (18 tema daha + özel) | `p2_ui::palette::{THEMES, palette_for}`, `Palette::for_theme(id)` |
| `src/motion.css` | `p2_ui::motion` (`M_FAST` … `EASE_POP`, `Entrance`) — Faz 0'dan beri birebir |
| `src/ui.css` (düğme, girdi, menü, modal, rozet, avatar, anahtar, sekme, toast…) | `p2_ui::components` (kısmen; aşağıdaki tablo) |
| `src/styles.css` `:root` yazı | `tokens::FONT`, `font_size::ROOT`, `LINE_HEIGHT_NORMAL`, `app.rs` kök öğesi |
| `@fontsource-variable/inter` | `crates/p2-ui/assets/fonts/InterVariable.ttf` (gömülü, `fonts::install`) |

Doğrulama: `cargo test -p p2-ui` — `palette::tests::butun_temalar_electron_ile_ayni` 19 temanın 38
renk token'ını (722 değer) Electron'un kendi `paletteFor()` çıktısıyla karşılaştırır. Çıktı
`node rust/tools/parity/export-palettes.mjs` ile `crates/p2-ui/src/palette_electron.json`'a yeniden
üretilir (theme.ts değişirse çalıştırın). Çalışan uygulamanın bütün CSS değişkenleri:
`node rust/tools/parity/dom-styles.mjs --screen login --tokens` → `C:/p2-pg/parity/dom/login-dark-tokens.json`.

## 1. Yazı — en büyük sapmalar buradaydı

| | Electron (hesaplanan) | Rust (bu fazdan önce) | Rust (şimdi) |
|---|---|---|---|
| Aile | `'Inter Variable'` (fontsource, Inter 4.x, wght 100–900) | "Segoe UI" | `tokens::FONT` = **"Inter Variable Text"** (gömülü TTF) |
| Kök boyut | 16px × **0,85** = **13,6px** (`--app-font-scale`) | GPUI varsayılanı 16px | `font_size::ROOT` (13,6) kökte |
| `--p2-fs-md` | 13 × 0,85 = **11,05px** | 13px | `font_size::MD` = 11,05 |
| Düz `font-size: 12px` | **10,2px** (`applyAppearance` her düz px'i ölçekler) | 12px | `tokens::fs(12.)` |
| `line-height: normal` | Inter: 1,2109 × boy | GPUI φ ≈ 1,618 | `LINE_HEIGHT_NORMAL` kökte |
| Ağırlık 650 (`.p2-btn`, başlıklar) | değişken eksen, gerçek 650 | – | tam 650 (`wght` ekseni yaması, `.css_weight(650.)`) — §6 |

Kurallar (ekran ajanları için):
- CSS'te `font-size: Npx` → Rust `fs(N)`; `var(--p2-fs-*)` → `font_size::*` (ikisi de ölçekli).
  `em`/`rem` değerleri 13,6px tabanlıdır.
- `line-height`, `padding`, `width/height`, `gap`, `border-radius` **ölçeklenmez** (px aynen).
- `line-height: 1.45` gibi oranlar → `.line_height(relative(1.45))`; `line-height: 18px` → `px(18.)`.
- `letter-spacing: .08em` → `.tracking(0.08)` (`p2_ui::P2Text`; yamalı DirectWrite, §6).
- Aile adı DirectWrite'ta **"Inter Variable Text"**dir. "Inter Variable" yazarsanız sessizce Segoe UI'ye
  düşer (`cargo run --release -p p2-ui --example fonts -- "<aile>"` ile denetleyin: ağırlık başına farklı
  FontId ve genişlik görmelisiniz).
- Mono: `FONT_MONO` = Cascadia Mono (Electron yığınının ilki; Windows 11'de kurulu). Emoji: `FONT_EMOJI`.

## 2. Renk token'ları

`Palette` alan adı = CSS değişkeni, `--p2-` öneki atılıp `-` → `_` (`--p2-text-2` → `p.text_2`).
Bütün `--p2-*` değişkenlerinin alanı vardır:

- Merdivenler: `navy_950…navy_500`, `slate_400…slate_100`, `white`, `blue_900…blue_100`
- Zemin: `bg_0..bg_3`, `bg_hover`, `hover_soft`, `bg_selected`, `glass` / `glass_strong` (`Gradient`), `overlay`
- Kenarlık: `border_subtle`, `border`, `border_strong`, `border_accent`, `line`
- Metin: `text`, `text_strong`, `text_2`, `text_3`, `text_4`, `text_accent`, `text_on_accent`, `neutral_fallback`, `link`
- Vurgu: `accent`, `accent_hover`, `accent_active`, `accent_soft`, `accent_soft_2`, `accent_grad` / `accent_bar` (`Gradient`),
  `accent_glow`, `accent_glow_strong`, `ring` (`Shadow`)
- Durum: `success*` (soft, border, text, bg, deep, mid, light), `text_on_success`, `danger*`, `warning*`
- Gölge: `inset_top`, `shadow_sm/md/lg` (`Vec<Shadow>`; GPUI'ye `p.shadow_lg()` ile, içe gölge dahil)
- Kimlik tonları: `hues[0..10]` (`bg`, `bg2`, `fg`) — P2 Mavi'de hue başına farklı doygunluk/açıklık (CSS sabitleri),
  diğer temalarda tek formül. `p.hue(i)` = (bg, fg).
- Rust'a özgü: `mention_bg`, `code_bg` — CSS'te tek bir token karşılığı yok; kullanıldıkları yerde ilgili
  CSS kuralından (`channel-content.css`) doğrulayın.

Tema seçimi: `settings.json` `theme` = `"dark"` (→ p2-mavi), `"light"` (→ **kagit**, Electron'un ilk açık
teması; P2 Mavi'nin açık eşi yoktur) ya da doğrudan tema kimliği (`gece`, `okyanus`, `sis`…). Özel tema
(`custom`, vurgu/zemin/ton/kontrast/canlılık) için `palette_for(&ThemeSpec{..})`. Ayarlar > Görünüm ızgarası
(19 kart + "Özel tema") henüz Rust'ta yok → settings ajanı.

Sabit (tema bağımsız) değerler: kaydırma çubuğu `rgba(120,170,255,.18)` / hover `.32`, 10px, iz saydam
(`theme.rs`'e işlendi); seçim `--p2-accent-soft-2`; odak halkası `--p2-ring` (`0 0 0 3px`).

## 3. Geometri ve katman

- `radius::{XS 6, SM 8, MD 11, LG 14, XL 18, PILL 999}` = `--p2-r-*`
- `space::{S1 4, S2 8, S3 12, S4 16, S5 20, S6 24, S8 32}` = `--p2-s-*`
- `z::{PANEL 40, DIALOG 90, MODAL 120, MENU 140, VIEWER 160, UPDATE 4000, TOAST 4200}` = `--p2-z-*`
  (GPUI'de `deferred(..).with_priority(..)` sırası olarak).
- `--p2-blur-sm 6px`, `--p2-blur 12px`: `backdrop-filter` → `p2_ui::fx::{BLUR_SM, BLUR}` + `.backdrop_blur(..)` (§6.8).
- `--p2-kicker-tracking .18em` = `KICKER_TRACKING_EM`.

## 4. Hareket (motion.css ↔ `p2_ui::motion`)

| CSS | Rust |
|---|---|
| `--m-fast 140ms`, `--m-base 220ms`, `--m-slow 360ms`, `--m-elastic .62s`, `--m-stagger 28ms` (tavan 360ms) | `M_FAST`, `M_BASE`, `M_SLOW`, `M_ELASTIC`, `M_STAGGER`, `M_STAGGER_MAX` |
| `--ease-out (.2,.8,.2,1)`, `--ease-in (.5,0,.8,.4)`, `--ease-elastic (.2,1.5,.35,1)`, `--ease-pop (.2,1.4,.4,1)` | `EASE_OUT`, `EASE_IN`, `EASE_ELASTIC`, `EASE_POP` |
| `p2-fade/rise/page/slide-left/right/pop-elastic/scale-in/…` | `Entrance::*` + `.p2_enter(id, Entrance, gecikme)` |
| `p2-pulse` (2.2s) | `motion::pulse()` |
| Etkileşim geçişleri: `background-color/border-color/color/box-shadow/opacity` `--m-fast --ease-out` (button, a, input, `.p2-row`, `.p2-card`) | GPUI'de hover anlık; geçiş için `with_animation` gerekir — ekran ajanları önemli yerlerde ekler |
| `prefers-reduced-motion` → bütün süreler 1ms | Ayarlar > Uygulama "hareketi azalt" ile aynı bayrak kullanılmalı |

Keyframe'lerin tam listesi (39 adet) çalışan uygulamadan: `dom-styles.mjs --tokens` çıktısında `keyframes`.

## 5. Temel bileşenler (ui.css → p2-ui)

Hepsi `p2_ui::controls` (ui.css ilkelleri), `p2_ui::form` (alanlar), `p2_ui::components` (logo, radyal zemin, cam kart),
`p2_ui::icons` / `p2_ui::glyphs` (ikonlar). Ölçüler ui.css + `dom-styles.mjs` ile doğrulandı.

| ui.css | Hesaplanan değerler (P2 Mavi, ölçek 0,85) | Rust |
|---|---|---|
| `.p2-btn` + `.primary/.secondary/.ghost/.danger` × `.sm/.lg/.icon/.pill` | h 34 (sm 28, lg 40), p 0 14 (10/18), r 8 (6/11), `650 fs-md`, gap 8; primary hover: brightness 1.08 + 1px yukarı + `0 10px 24px` | `controls::button(id, BtnVariant, BtnSize, icon, pill, &p).child(..)` |
| `.p2-input/.p2-select/.p2-textarea` | p 9 12, r 8, bg-3, `500 fs-md`; focus `border-accent` + `ring` | `form::input(&state, InputKind::P2, &p, window, cx)` |
| `.auth-card input` / kurulum alanları | h 40, p 13 14, r 11, `600 12px×.85` (etiketten kalıtılır) | `form::input(.., InputKind::Auth, ..)`, `form::auth_label(etiket, alan, &p)` |
| yerel onay kutusu (`accent-color`) | 13×13, r 2, accent dolgu + beyaz tik (piksel ölçümü) | `form::native_checkbox(checked, &p)` |
| `.p2-toggle` | 36×20, topuz 16, `translateX(16px)` 220ms ease-pop | `form::toggle(id, on, epoch, &p)` (`epoch` değişince canlanır) |
| radyo | 16px halka, seçili 8px nokta | `form::radio(selected, &p)` |
| `.p2-modal-layer` + `.p2-modal(.sm/.lg/.xl)` + head/body/foot | katman `overlay` (dim: `rgba(2,4,10,.82)`), p 38 24 24; kart min(590,100vw−48), r 18, `glass-strong`, `shadow-lg`, `p2-pop-elastic` | `controls::{modal_layer, modal, modal_head, modal_body, modal_foot}` — canlandırma çağıranda (`.p2_enter`) |
| `.p2-menu` / `.p2-menu-item(.danger)` / `hr` / `.p2-menu-label` | min-w 200, p 6, r 11, `glass`, `shadow-lg`, `p2-scale-in`; öğe 32px, gap 10, p 0 10, r 6, `550 fs-md`; ayraç 1px 5/4 | `controls::{menu, menu_item(id, etiket, ikon, kısayol, danger, &p), menu_separator, menu_label, submenu_chevron}` |
| `.p2-badge(.soft/.danger/.success/.warning/.muted)` | h 18, min-w 18, p 0 6, hap, `700 fs-xs` | `controls::badge(metin, BadgeTone, &p)` |
| `.p2-pill(.active)` | h 26, p 0 10, `600 fs-sm` | `controls::pill(id, active, &p)` |
| `.p2-kicker` | `800 fs-2xs`, .18em, büyük harf | `controls::kicker(..)` |
| `.p2-kbd` / `.p2-keys` | min-w 20, p 2 7, alt kenarlık 2px | `controls::{kbd, keys}` |
| `.p2-avatar` + `.p2-avatar-status` | köşe %25, 145° ton gradyanı, `800` boy×.36, .02em; nokta 11×11, 2px `bg-1` halka | `controls::{avatar_box(size, hue, round, &p), status_dot(Presence, ring, &p)}` (+ `components::avatar` baş harf/görsel) |
| `.p2-tabs(.line/.segment)` + `.p2-tab` | h 30, p 0 12, r 8, `650 fs-md`; line: 2px `accent-bar` alt çizgi | `controls::{tabs(TabsStyle, &p), tab(id, etiket, active, style, epoch, &p)}` |
| `.p2-card(.glow/.pad)` | r 14, `glass`, `shadow-md` + inset | `controls::card(glow, pad, &p)` |
| `.p2-row(.active)` | min-h 36, p 0 10, r 8, `550 fs-md`; hover 2px sağa | `controls::row(id, active, &p)` |
| `.p2-progress(.thin/.indeterminate)` | 4px (2px), `navy-600` iz, `accent-bar`; belirsiz %38 1,3 sn | `controls::progress(Some(0..1) / None, thin, &p)` |
| `.p2-divider`, `.p2-empty` | | `controls::{divider, empty}` |
| `.p2-toast(.ok/.error)` | 352px, `44px 1fr auto`, gap 13, p 12 12 14 14, r 14, `glass`; elastik giriş, 0,25 sn parlama, 2px süre çubuğu | `controls::toast(..)`; uygulama yığını `p2-client/src/toasts.rs` (`toasts::push(ToastTone, başlık, metin)`) |
| `.p2-logo(.xs…xl)` | `src/assets/p2-icon.png`, köşe %25, `accent-glow-strong` + 1px halka | `components::logo_image(size, &p)` (`p2_icon()` görseli) |
| yerel `title` ipucu | Windows ipucu (12px Segoe UI) | `controls::tooltip(metin, w, cx)` → `.tooltip(..)` |
| ikonlar (ComposerIcons/ChannelIcons çizgi, CallIcons dolgu) | `currentColor`, kontur CSS'ten | `icons::icon("bell").stroke(1.8).el()` / `icons::icon("call:mic").filled().el()`; `glyphs::{named, filled, ui}` (pencere düğmeleri, check, chevron, ok, göz) — **renk svg'nin kendisine** `.text_color(..)` verilmeli (GPUI svg rengi kalıtmaz) |
| kaydırma çubuğu | 10px, thumb `.18`/`.32`, hap | `theme.rs` ✓ |

Kabuk (A): `p2-client/src/titlebar.rs` (38px başlık çubuğu, ☰ → X 220ms ease-pop, `.app-menu`, pencere düğmeleri
`WindowControlArea` ile yerel isabet testi), `update_banner.rs` (indirme şeridi + "Yeni sürüm hazır" penceresi),
`splash_view.rs` (Yükleniyor ekranı), `screens/setup_ui.rs` (`.setup-page/.setup-shell/header`, `mode_back`, `install_button`).
Başlık çubuğu sayfa adı: `titlebar::set_project_title(ad)` (çalışma alanı seçili projede çağırır).

## 6. GPUI sınırları — çözümler (A ajanı) ve kalanlar

Çözülenler (herkes kullanabilir):
1. **Harf aralığı** — `third_party/gpui-pre-windows` yaması: FontFeatures'taki sahte `P2ls` etiketi (em×1000)
   DirectWrite `IDWriteTextLayout1::SetCharacterSpacing` (karakter sonu, Chromium gibi). Kullanım:
   `use p2_ui::P2Text as _; div().tracking(-0.06)` (`letter-spacing: -.06em`) ya da `.tracking_px(px, font)`.
   Liga/clig kapanır (Chromium da aralıklı metinde kapatır). Kalıtılır (CSS gibi).
2. **Ara ağırlıklar** (650, 550, 750, 850) — aynı yama değişken Inter'in `wght` eksenini tam değere koyar
   (`IDWriteTextLayout4::SetFontAxisValues`, örneklenmiş yüz ayrı FontId olarak taranır). `.css_weight(650.)`
   ya da `.font_weight(FontWeight(650.))`.
3. **`font-optical-sizing: auto`** — yama `opsz` eksenini yazı boyuna (px) ayarlar; büyük başlıklar Chromium'daki
   "Display" kesimiyle çizilir. Ek kod gerekmez.
4. **Yazı yumuşatma** — yama ClearType'ı kapatır, Chromium gibi gri tonlama (`P2_CLEARTYPE=1` eskiye döndürür).
5. **`line-height: normal`** — Chromium satır kutusu `round(asc×fs)+round(desc×fs)`: `p2_ui::text::normal_line_height(fs)`
   (10,2→12, 11,05→14, 25,5→31). Kök `LINE_HEIGHT_NORMAL` oranı yuvarlamayı yapmaz; ölçünün önemli olduğu yerde bunu verin.
6. **Radyal / çok duraklı gradyan** — `components::radial_background(w, h, (x%, y%), &[(renk, durak)…], zemin)`:
   resvg ile SVG görseli (farthest-corner, Chromium gibi). Alfalı duraklar önce zeminle birleştirilir (premultiplied
   karışımın aynısı); `transparent`'a giden gradyan için son durak = zemin rengi. Çok renkli SVG için `icons::svg_image`.
7. **CSS geçiş/animasyon** — `p2_ui::motion` (`EASE_*`, `CubicBezier::ease`, `Entrance` + `.p2_enter(id, kind, gecikme)`);
   özel kareler için `with_animation(id, Animation::new(süre), |el, t| ..)` + `EASE_POP.ease(t)`. Örnekler: ☰ → X dönüşü
   (`titlebar.rs`, svg `with_transformation(rotate)`), topuz kayması (`form::toggle`), parlama şeridi (`setup_ui::shine`,
   `controls::toast`), hata kutusu `p2-rise 170ms` (`auth.rs`). DİKKAT: `p2_enter` öğeyi `relative` yapar — mutlak
   konumlu katmanı bir `div().absolute()…` sarmalayıcıda canlandırın.

8. **Ajan K — yamalı gpui-pre çekirdeği** (`third_party/gpui-pre`, `p2_ui::fx`):
   - `backdrop-filter: blur(N) [saturate(S)]` → `.backdrop_blur(fx::BLUR | fx::BLUR_SM)` / `.backdrop(N, S)`. DirectX çizicisinde
     gerçek iki geçişli Gauss (σ = N, Chromium gibi öğe kutusu içinde ayna kenar), yuvarlak köşe + opaklık. `controls::{menu, modal_layer}`
     ve ekranlardaki bütün `p.overlay`/dim katmanları + Electron'da bulanık olan cam menüler uygulandı.
   - Başlık çubuğu modal katmanının ÜSTÜNDE kalır (Electron `.titlebar` z-index 1000 > katman 120; ekran görüntüsüyle doğrulandı).
   - `transform` → `.scale(s)`, `.scale_x(s)`, `.rotate(deg)` (yalnız SVG/glif), `.translate(x, y)`, `.transform_origin(ox, oy)`; hover/group_hover
     içinde de kullanılır ve geçer. `Entrance::{PopElastic, ScaleIn, ScaleInFrom, Page, LogoPop, SuccessPop}` artık gerçek ölçek kullanır.
   - `filter: grayscale()/brightness()` → `.grayscale(g)`, `.brightness(k)` (renk matrisi; görsellerde gri bayrağı).
   - `transition`: `.id()`'li ve hover/active/group_hover stili olan HER öğe varsayılan 140ms ease-out geçer (ui.css). Öğeye özel
     `.transition(süre, eğri)`, kapatmak için `.no_transition()` (`p2_ui::P2Transition`). Durum (sınıf) değişimi için `fx::tween` / `tween_raw`
     + `keep_animating` (ör. kategori oku döndürmesi).
   - Kaydırma çubuğu: taşan her `overflow_*_scroll` div ve `list()` otomatik 10px Chromium çubuğu çizer (başparmak .18/.32, sürüklenebilir).
     Kendi çubuğunu çizen alan `.no_auto_scrollbar()`. Not: Chromium çubuğu 10px yer ayırır, GPUI ayırmaz (içerik çubuğun altına uzanır).
   - Yazı boyutu ayarı (70–125): `fx::set_font_scale_percent` → bütün px yazı boyları canlı çarpılır (px satır yükseklikleri sabit kalır).
   - `text-shadow` → `fx::text_shadow(make, dx, dy, blur, renk)` (Gauss ağırlıklı kopyalarla yaklaşık).
   - Yer tutucu: gpui-component girdileri tema `muted_foreground` = `text-4` (`.p2-input::placeholder`).

Kalan (GPUI'de yapılamayan ya da yapılmayan):
1. **Dörtgen döndürme** (`.library-gear` zemini, `.board-card.dragging`): dörtgen gölgelendiricisi eksene hizalı; yalnız SVG/glif döner.
2. **Kaydırma çubuğu yer ayırma** (yukarıda) ve `list()` çubuğunun sürüklenmesi.
3. **Yerel girdi yer tutucu**: kuralsız auth alanlarında Chromium #757575; Rust'ta tüm girdiler tek renk (`text-4`).
4. **Glif kenarları**: gri tonlamayla bile Skia ↔ DirectWrite glif konumları alt pikselde ayrışır; metin yoğun ekranlarda
   %1–2 kalan uyumsuzluk bundandır.
