# Görsel parite aracı (Electron ↔ Rust)

Amaç: Rust/GPUI istemcisinin Electron istemcisiyle **piksel düzeyinde aynı** görünmesi. Electron referanstır.
Token/yazı tipi eşlemesi: [`TOKENS.md`](TOKENS.md). Ekran listesi: [`catalog.json`](catalog.json).

## Önkoşullar

- Test PostgreSQL `:55432` + test sunucusu `:4110` (NestJS; `P2_API` ile Rust sunucusu da olur). Kapatmayın.
- Rust istemcisi derli: `cd rust && cargo build --release -p p2-client -p p2-wincap` (`C:/p2-rust-target/release`).
- Depo kökünde `node_modules` (electron, vite, pngjs).

## Komutlar (depo kökünden)

```bash
# Koşu: ekran(lar) × tema → C:/p2-pg/parity/runs/<zaman>/report.html ; özet + geçmiş: C:/p2-pg/parity/index.html
node rust/tools/parity/parity.mjs --screens login,workspace-chat --themes dark,light
node rust/tools/parity/parity.mjs --group settings          # auth | workspace | people | settings | admin | tools | library
node rust/tools/parity/parity.mjs --list                    # katalog
#   --threshold 24 (kanal farkı eşiği)  --settle 1200 (ms)  --skip-seed

# Electron hesaplanmış stilleri (her görünür öğe: yazı, renk, boşluk, kenarlık, gölge, geçiş/animasyon, ::before/::after)
node rust/tools/parity/dom-styles.mjs --screen settings-appearance --root ".settings-layer" [--tokens]
#   → C:/p2-pg/parity/dom/<ekran>-<tema>[-kök].json + .txt (girintili tek satırlık özet)

# Etkileşimli keşif (yeni ekran adımı yazarken): Electron açık kalır, komutlar ona bağlanır
node rust/tools/parity/session.mjs start [--theme light] [--name s1]   # arka planda bırakın
node rust/tools/parity/session.mjs screen message-menu                 # katalog ekranına git
node rust/tools/parity/session.mjs eval "document.querySelector('.p2-menu').outerHTML"
node rust/tools/parity/session.mjs steps '[{"rightClick":".channel-row::text(genel)"}]'
node rust/tools/parity/session.mjs shot C:/p2-pg/parity/x.png
node rust/tools/parity/dom-styles.mjs --session default --root ".channel-context-menu"
node rust/tools/parity/session.mjs stop

node rust/tools/parity/lib/seed.mjs            # tohum verisi (koşu bunu kendisi çağırır)
node rust/tools/parity/export-palettes.mjs     # theme.ts değişince Rust palet testinin beklenenleri
```

Paralel ajanlar aynı anda koşabilir: her koşu kendi Vite/CDP portunu ve geçici profillerini (`C:/p2-pg/parity/profiles/…`)
alır. `session.mjs`'de `--name` farklı olmalı.

## Nasıl çalışır

- **Electron**: `electron-main.cjs` geçici bir uygulama klasöründen `electron/main.cjs`'i geliştirme modunda
  yükler; renderer Vite'tan (`127.0.0.1:<rastgele>`), API `localStorage p2:apiUrl` ile test sunucusuna.
  Yalıtım: userData/sessionData geçici profil (APPDATA altı reddedilir), `p2://` kaydı, kısayol onarımı,
  `schtasks/sc/powershell/taskkill/reg` çağrıları, bildirimler ve güncelleyici etkisiz; pencere odak
  çalmadan açılır (`showInactive`), odak CDP ile taklit edilir. Kapatma `taskkill /T` ile — `before-quit`
  (sunucu durdurma kodu) hiç çalışmaz. Ölçek: `--force-device-scale-factor=1`, pencere 1280×800.
  Gezinme ve görüntü yalnız CDP ile (kendi renderer'ımıza `Input.*`); masaüstüne girdi gitmez.
- **Rust**: `p2-client.exe` geçici `P2_PROFILE_DIR` (settings.json: sunucu, dil `tr`, tema) +
  `P2_AUTOPILOT_*` + `P2_NO_AUDIO_DEVICE=1`, `P2_SILENT_NOTIFY=1`, `P2_NO_TRAY=1`, `P2_SPLASH=0`,
  `P2_NO_ELECTRON_IMPORT=1`. Hazır olunca (stdout iz satırı, `rust.ready`) `p2-wincap --client <pid>` istemci
  alanını (çerçevesiz, 1280×800) yakalar. Gerekirse `rust.steps` ile **yalnız kendi penceresine**
  PostMessage: `--click/--rclick/--move` (p2-wincap).
- **Fark**: piksel başına en büyük kanal farkı > eşik → uyumsuz. Rapor: Electron | Rust | fark, kaydırıcılı
  üst üste görünüm, bölge tablosu (`catalog.regions`), en kötü 40×40 karolar. `history.json` ekran-tema
  başına zaman serisi tutar (ilerlemeyi gösterir).
- **Tohum**: ilk projede `GENEL › #parite` kanalı (kalın/italik/üstü çizili/bağlantı, @bahsetme, satır içi kod
  + kod bloğu, alıntılı yanıt + liste, 👍×2 🎉 tepkiler, iş parçacığı yanıtı, PNG ve metin eki, uzun Türkçe
  paragraf). **Bu kanala yazan adım eklemeyin** (diğer ekranların görüntüsü kayar). Yazan senaryolar kendi
  kanalını açsın.

## Katalog adımları

Electron (`electron.steps`): `{"use":"makro"}` (macros: `ws`, `parite`, `logout`, `settings`, `admin`),
`{"login":true}`, `{"logout":true}`, `{"reload":true}`, `{"wait":"seçici","timeout":ms}`, `{"waitGone":…}`,
`{"click":…}`, `{"dblclick":…}`, `{"rightClick":…}`, `{"hover":…}`, `{"clickAt":[x,y]}`, `{"type":["seçici","metin"]}`,
`{"insert":"metin"}`, `{"key":"ctrl+,"}` (Enter, Escape, Tab, Arrow*, Backspace, harf + ctrl/shift/alt),
`{"sleep":ms}`, `{"eval":"js"}`, `{"mouseAway":true}`, `{"session":{"p2:parity-api":"…"}}` (sessionStorage; yeniden
yüklemede API/kurulum türünü geçersiz kılar), `{"mock":{"url":"*/auth/login","status":401,"body":{…}}}` (tek seferlik sahte yanıt).
Her adıma `"optional":true` eklenebilir.

Kayma tuzakları (2026-11-13):
- Electron profili koşu boyunca aynı kalır; `p2:last-view` son açık sohbeti hatırlar ve girişten sonra **zaman uyumsuz**
  geri yükler. `ws` makrosu bu yüzden önceki ekranın sohbetine (ör. Rust Bot DM'i) iner; tek tık `parite`'yi de ezebilir.
  `parite` makrosu tıklar, `.channel-row.active::text(parite)` bekler, bir kez daha tıklar. Kanal/sohbet önemliyse `ws` değil `parite` kullanın.
- `click`/`rightClick` hedefi `scrollIntoView` ile görünüre kaydırır; `::nth(0)` DOM'daki ilk görünür (boyutlu) eşleşmedir,
  görüş alanında olmayabilir (medya tohumu akışı uzatınca `profile-panel` akışı yukarı kaydırıyordu). Görüş alanındaki öğeyi
  `eval` ile işaretleyip (`data-parity-target`) onu tıklayın.
- Menüden açılan pencerelerde fare son tıklanan yerde kalır: `mouseAway` ekleyin (yoksa altındaki kart hover görünür).
- Rust'ta zaman uyumsuz dolan bölümler için `ready`'den sonra iz satırı bekleyin (`waitLine`), sabit `sleep` değil.

Seçiciler: CSS; `css::text(Metin)` (innerText/aria-label/title içeren en küçük görünür öğe); `text=Metin` (tam metin);
sonda `::nth(i)` (i. görünür eşleşme).

Rust (`rust`): `env` (ortam), `use` (rustMacros), `ready` (stdout regex; ör. `sohbet: #parite \d+ mesaj yüklendi`),
`login:false`, `settings:false` (kurulum türü ekranı), `api` (adres geçersiz kılma), `steps`
(`{"click":[x,y]}`, `{"rightClick":[x,y]}`, `{"move":[x,y]}`, `{"key":"0x1B"}`, `{"sleep":ms}`, `{"waitLine":"re"}`),
`settle`, `timeout`, `status:"missing"` (Rust'ta kanca/özellik yok).

Bu fazda eklenen Rust görünüm kancaları (veri değiştirmez): `P2_AUTOPILOT_AUTH_VIEW=register|2fa`,
`P2_AUTOPILOT_PROJECT_MENU=1`, `P2_AUTOPILOT_CHANNEL_MENU=<kanal>[@x,y]`, `P2_AUTOPILOT_SCROLL=top`,
`P2_AUTOPILOT_DRAFT=<metin>`, `P2_AUTOPILOT_EMOJI_PANEL=1`, `P2_AUTOPILOT_MENTION_POPUP=<sorgu>`,
`P2_AUTOPILOT_APP_MENU=1` (☰ menü açık), `P2_AUTOPILOT_TOAST=info,ok,error` (örnek kartlar), `P2_AUTOPILOT_UPDATE=downloading:42[:sürüm]|ready[:sürüm]`.
Yalnız Rust görüntüsü (Electron'un otomatik açamadığı elle ekranlar): `node rust/tools/parity/rust-shot.mjs --login false --ready "ekran: giriş" --env K=V --out x.png`.
İkon verisi: `node rust/tools/parity/gen-icons.mjs` (ComposerIcons/ChannelIcons/CallIcons.tsx → `p2-ui/src/icons_data.rs`).
Var olanlar: `P2_AUTOPILOT_CHANNEL/DM/OPEN_GROUP/PROFILE/SETTINGS/ADMIN(open:…)/PAGE/USER_MENU/ACTIVITY/SEARCH/BROWSER`.
Yeni ekran eklerken aynı biçimde **yalnız görünüm açan** kanca ekleyin (mevcut `P2_AUTOPILOT_THREAD/GROUP/MANAGE/MENTION/EMOJI` veri yazar).

## Kurallar

- Masaüstüne sentetik girdi yok (SendKeys/SendInput/SetCursorPos). Yalnız CDP (kendi Electron'umuz) ve p2-wincap PostMessage (kendi Rust penceremiz).
- Gerçek ses aygıtı yok: arama ekranları `manual`; Rust tarafı `P2_NO_AUDIO_DEVICE=1` ile. Electron'da `getUserMedia` açan hiçbir adım yazmayın.
- Kullanıcının `%APPDATA%\P2`, `%APPDATA%\P2-Rust`, kurulu uygulamaları, `p2://` kaydı, :4110/:55432 süreçleri — dokunulmaz.

## Bölüşüm (paralel parite ajanları, dosya sahipliği)

Bir dosyayı yalnız sahibi düzenler. `p2-ui` paylaşımlıdır: `tokens.rs`/`theme.rs`/`palette.rs`/`fonts.rs`/`components.rs`
sahibi **A**; diğerleri ortak bileşen gerekiyorsa `crates/p2-ui/src/<kendi_adı>.rs` yeni dosya açıp `lib.rs`'e tek satır ekler.
`catalog.json`'da yalnız kendi `owner` ekranlarınızı düzenleyin.

| Ajan | Ekranlar (catalog `owner`) | Dosyalar |
|---|---|---|
| A — Kabuk + giriş + tasarım sistemi | login, register, two-factor, mode-picker, server-problem, setup-admin, onboarding, splash, app-menu, toasts, update-banner | `app.rs`, `main.rs`, `shell.rs`, `splash.rs`, `updates.rs`, `screens/{auth,mode,setup,problem,onboarding,launch_splash,auto_update}.rs`, `crates/p2-ui/*` (ortak) |
| B — Çalışma alanı iskeleti + sohbet akışı | workspace-chat, workspace-chat-top, message-hover, channel-browser | `screens/workspace.rs`, `screens/chat_tools.rs` |
| C — Sohbet satırı, yazma kutusu, zengin metin | composer-draft, emoji-picker, mention-popup, message-menu, channel-search, thread-panel | `screens/{chat,chat_extras,thread}.rs`, `crates/p2-ui/src/rich.rs` |
| D — Kişiler | profile-panel, activity, dm-chat, group-chat, group-creator, user-menu | `screens/{group_members,group_creator,activity_ui}.rs`, `avatars.rs`; workspace.rs'deki profil/kullanıcı menüsü kodunu `screens/people_ui.rs`'e taşıyarak (B ile bir kez anlaşın) |
| E — Ayarlar | settings-* (9 sekme) | `screens/{settings,settings_app,settings_media,settings_library}.rs` |
| F — Yönetim | admin-* (8), manage-create-channel, channel-permissions, new-project, channel-menu, project-menu | `screens/{admin,channel_perms,manage_ui}.rs` |
| G — Araçlar | board, documents | `screens/{board,board_editor,documents}.rs` |
| H — Kütüphane | library, library-notes, library-screenshots, project-hub | `screens/{library,overlay_ui}.rs` (+ yeni notes/screenshots/hub dosyaları) |
| I — Arama (manual) | call-voice-room, call-incoming, screen-picker, call-widget | `call.rs`, `call_widget.rs`, `screens/{call_ui,screen_picker}.rs`, `annotate.rs`, `overlay_draw.rs` |

Sıra önerisi: önce A (yazı/bileşen temeli; diğer herkesi etkiler) ve B (iskelet: ray 72px, kenar çubuğu 252px,
başlık 38px — bütün `workspace` düzenli ekranların bölge ölçüleri buna bağlı); sonra diğerleri paralel.
