# P2 — Rust

Electron + React + NestJS yığınından Rust'a geçiş. Plan: [`docs/rust-gecis-plani.md`](../docs/rust-gecis-plani.md).

Durum: **Faz 0 (teknik keşif) tamamlandı** — dört spike da geçti. İstemci geçişi: Dilim 1
(giriş akışı), Dilim 2 (çalışma alanı, sohbet, gerçek zamanlı) , Dilim 3 (DM, gruplar,
profil, ayarlar), Dilim 4 (sesli arama, sesli odalar, ekran paylaşımı), Dilim 5 (görev panosu, dokümanlar, etkinlik), Dilim 6 (kütüphane / yama motoru), Dilim 7 (oyun içi overlay) , Dilim 2b (yanıt, iş parçacığı/forum, ekler, bahsetme, emoji, iletme, kanal tarayıcısı) , Dilim 4c (kamera, yayın kalitesi, moderasyon, kişi başına ses) , Dilim 8 (yönetim paneli, roller, erişim, kanal izinleri, proje/kanal yönetimi) , Dilim 9 (uygulama kabuğu: tek örnek, tepsi, kapatma, bildirim) , Dilim 10 (profil fotoğrafı, yıldızlılar, kanal bildirimi, arama, geçmiş, görüntüleyici, grup üyeleri) ve Dilim 11 (Kütüphane ve Uygunluk ayarları) tamam. Sunucu (Faz 2) ve **Faz 8** (kurulum paketleri, güncelleme yayını, Electron → Rust taşıması) tamam.

## Gereksinimler

- Rust (stable, MSVC): `winget install Rustlang.Rustup`
- Visual Studio C++ derleme araçları (MSVC + Windows SDK)

Derleme çıktısı OneDrive dışında, `C:\p2-rust-target` altında tutulur (`.cargo/config.toml`).
Depo OneDrive'da olduğu için senkronizasyon `target/` klasörünü kilitliyor ve yavaşlatıyordu.

## Çalıştırma

```bash
cd rust
cargo run -p p2-client --release     # P2 istemcisi (Rust) — bugünkü sunucuna bağlanır
cargo run -p p2-showcase --release   # Spike 1: GPUI arayüz vitrini
cargo test --workspace               # birim testleri
```

## Yapı

| Yol | İçerik |
|---|---|
| `apps/p2-client` | **P2 masaüstü istemcisi (Rust)** — Dilim 1: kurulum türü, sunucuya bağlanma, bağlanamadık, ilk yönetici, giriş/kayıt/2FA, kuruluş oluştur/katıl. Dilim 2: proje rayı, kanallar/kategoriler/okunmamışlar, sohbet (yaz/düzenle/sil/tepki, eski mesajları kaydırarak yükleme), kişiler ve çevrimiçi durum, gerçek zamanlı olaylar, kullanıcı menüsü. Dilim 3: DM'ler, gruplar (kur/ayrıl), profil paneli, durum seçimi, ayarlar (hesap, 2FA, tema, dil, kısayollar). Kendi profilini kullanır (`%APPDATA%\P2-Rust`) |
| `packaging/` | **Faz 8:** Windows kurulumları (`build-installers.mjs`, `nsis/p2-installer.nsi`), doğrulama (`verify-installer.mjs`), yayın (`publish.mjs`), Linux sunucu paketi (`linux/build-linux-server.mjs`) |
| `crates/p2-client-core/src/migrate.rs` | Electron → Rust taşıma (ayarlar, oturum, kütüphane); Electron tarafı `electron/migrate-bridge.cjs` |
| `crates/p2-patcher` | Kütüphane motoru: içerik tanımlı parçalama (cdc.cjs ile aynı), manifest/kurulum planı, indir-kur-doğrula-onar, oyun başlat, sürüm yükle |
| `apps/p2-overlay` | Oyun içi overlay host'u (oyun süreci başına): ekran dışı GPUI panel, bildirimler, HUD, görev/not/performans/arama araçları, ekran görüntüsü |
| `crates/p2-overlay-client` | İstemci tarafı overlay yöneticisi: enjektörle başlatma, alt süreç izleme/enjeksiyon, host süreçleri ve JSON protokolü |
| `crates/p2-media` | Arama motoru: WebRTC örgü (libwebrtc), mikrofon/hoparlör (cpal), yankı/gürültü/kazanç (APM), zil ve işaret sesleri, ekran paylaşımı (DesktopCapturer), gelen görüntü |
| `crates/p2-client-core` | API istemcisi (401'de tek yenileme), oturum (DPAPI şifreli), yerel ayarlar, gerçek zamanlı bağlantı (socket.io `/stream`) |
| `apps/p2-wincap` | Test aracı: bir sürecin penceresini Windows Graphics Capture ile PNG'ye alır (girdi göndermez) |
| `crates/p2-i18n` | 11 dil, `src/locales` sözlüklerinden (`tools/i18n-export.mts`) |
| `crates/p2-ui` | Tasarım sistemi: P2 Mavi token'ları, tema, hareket (`motion.css` karşılığı), ortak bileşenler |
| `apps/p2-showcase` | Faz 0 / Spike 1 vitrini: 10.000 mesajlık sanal liste, zengin metin, IME, modal, toast, sayfa geçişi, tema |
| `apps/p2-server` | Faz 0 / Spike 4 sunucu iskeleti: `/auth/*`, `/setup/*`, socket.io `/stream` çevrimiçi durum — NestJS ile birebir |
| `crates/p2-overlay-ipc` | Oyun içi overlay paylaşımlı bellek protokolü (C++ `p2overlay_ipc.h` şema 4 karşılığı) |
| `apps/p2-overlay-host` | Faz 0 / Spike 3: Rust overlay host — bugünkü DLL'e GPUI ile ekran dışı çizim (`p2-overlay-host d3d11\|d3d12`) |
| `apps/p2-d3dtest` | Overlay testi için küçük D3D11/D3D12 "oyun" penceresi |
| `apps/p2-callbot` | Faz 0 / Spike 2: Rust WebRTC arama katılımcısı (libwebrtc + socket.io), P2 arama protokolüyle |
| `tools/` | `contract.mjs` (sunucu sözleşme testi), `spike-server.ps1`, `spike2/` (Electron ↔ Rust arama testi), `client-smoke.ps1` (Dilim 1), `slice2-test.mjs` … `slice7-test.mjs`, `slice2b-test.mjs`, `slice4c-test.mjs`, `slice8-test.mjs`, `slice8b-test.mjs`, `slice9-test.mjs`, `slice10-test.mjs`, `slice11-test.mjs`, `slice12-test.mjs`, `slice13-test.mjs` (Dilim 2–13, 2b, 4c uçtan uca), `faz8-test.mjs` (yayın + Electron → Rust taşıması) |
| `tools/parity/` | **Görsel parite (Faz 9)**: Electron ↔ Rust yan yana koşu, fark yüzdesi + HTML rapor (`parity.mjs`), ekran kataloğu (`catalog.json`), Electron hesaplanmış stil çıkarıcı (`dom-styles.mjs`), etkileşimli CDP oturumu (`session.mjs`), token eşlemesi ([`TOKENS.md`](tools/parity/TOKENS.md)). Kullanım: [`tools/parity/README.md`](tools/parity/README.md) |
| `third_party/` | Yerel yamalı bağımlılıklar (bkz. içindeki README) |

Sunucuyu çalıştırmak (mevcut P2 şemasına sahip bir PostgreSQL gerekir):

```bash
DATABASE_URL=postgresql://... JWT_ACCESS_SECRET=... JWT_REFRESH_SECRET=... PORT=4000 cargo run -p p2-server --release
```

İstemci tanılama: `P2_TRACE_SOCKET=1` (gerçek zamanlı bağlantı aşamaları, stderr),
`P2_TRACE_RENDER=1` (saniyede görünüm çizimi), `P2_PROFILE_DIR=<klasör>` (ayrı profil).

## Paketleme ve yayın (Faz 8)

Gereken araçlar bu makinede zaten var: makensis + eklentiler electron-builder önbelleğinde
(`%LOCALAPPDATA%\electron-builder\Cache`), StdUtils/UAC başlıkları ve WinSW `node_modules`'ta, overlay
enjektörü/DLL'i `native/overlay/dist`'te (`npm run overlay:collect`). Komutlar depo kökünden:

```bash
# 1. Kurulum paketleri (önce cargo build --release; sürüm package.json'dan ya da --version ile)
node rust/packaging/build-installers.mjs --version 0.11.0               # istemci + sunucu
node rust/packaging/build-installers.mjs --product client --skip-cargo  # var olan ikililerle
#   → C:\p2-rust-target\dist\client\P2-Client-Setup-0.11.0.exe + latest.yml
#   → C:\p2-rust-target\dist\server\P2-Server-Setup-0.11.0.exe + latest.yml

# 2. Doğrulama — kurulumu ÇALIŞTIRMADAN paketi açar: dosyalar, kayıt anahtarları, argümanlar, latest.yml
node rust/packaging/verify-installer.mjs

# 3. Yayın: sunucunun güncelleme klasörüne (P2_UPDATE_DIR; yoksa %ProgramData%\P2\updates)
node rust/packaging/publish.mjs --version 0.11.0 --skip-build

# 4. Uçtan uca (test PostgreSQL :55432): yayın + sunum + Electron → Rust taşıması
node rust/tools/faz8-test.mjs

# Linux sunucu paketi (Ubuntu 24.04): Linux'ta ya da cargo-zigbuild ile derler; yoksa hazır ikiliyle
node rust/packaging/linux/build-linux-server.mjs --binary <linux-x64 p2-server>
```

Kurulumlar Electron'unkilerle aynı kimliği taşır ("P2 Client" / "P2 Server", aynı kayıt anahtarları,
klasör, kısayollar, AppUserModelID); kurulu Electron sürümünün üzerine yerinde yükseltir ve
güncelleyicinin `--updated /S --force-run` argümanlarıyla sessiz çalışır. Ayrıntılar ve bilinçli
farklar: [plan §9, Faz 8 notları](../docs/rust-gecis-plani.md).

**Electron'dan geçiş sırası:** (1) Electron 0.10.x köprü sürümünü (`electron/migrate-bridge.cjs`
içerir) `npm run publish:update` ile yayınla; açılan her istemci `%APPDATA%\P2\migrate.json` yazar.
(2) Rust sürümünü daha büyük bir sürümle (`--version 0.11.0`) paketleyip `publish.mjs` ile yayınla.
Electron güncelleyicisi onu indirip kurar; Rust istemcisi ilk açılışta ayarları, oturumu (yeniden giriş
yok) ve kütüphaneyi `%APPDATA%\P2-Rust`'a alır. Electron profili olduğu gibi kalır.

Taşıma ortam değişkenleri: `P2_ELECTRON_PROFILE_DIR` (kaynak Electron profili; testler),
`P2_NO_ELECTRON_IMPORT=1` (kapat). `P2_PROFILE_DIR` verilmişse gerçek `%APPDATA%\P2` hiç okunmaz.
`p2-client --p2-import-electron` yalnız taşımayı yapıp raporu yazar.

## Faz 0 spike'ları

Sonuçlar ve ölçümler: [`docs/rust-gecis-plani.md` §8](../docs/rust-gecis-plani.md).

| # | Konu | Durum |
|---|---|---|
| 1 | GPUI arayüz vitrini + ölçüm | ✅ ~97 MB, simge durumunda %0,3 CPU |
| 2 | WebRTC: Electron 0.9.9 ile aynı aramaya girme | ✅ 7/7: iki yönde ses + görüntü (`apps/p2-callbot`, `tools/spike2`) |
| 3 | Overlay: mevcut C++ DLL'e Rust'tan çizim | ✅ D3D11 + D3D12: GPUI ekran dışı çizim, kısmi kare, girdi (`apps/p2-overlay-host`) |
| 4 | Axum + sqlx sunucu iskeleti | ✅ NestJS ile sözleşme testi 72/72, 4,6× istek/sn |
