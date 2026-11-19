# P2

P2; ekipler, oyuncu toplulukları ve arkadaş grupları için self-hosted masaüstü iletişim ve çalışma platformudur. Kanallar, doğrudan/grup mesajları, sesli görüşmeler ve ekran paylaşımını; organizasyon, rol ve proje düzeniyle birleştirir.

Oyun geliştiren ekipler için ayrıca görev panoları, belgeler, build dağıtımı, içerik tanımlı güncellemeler ve ortak oyun kütüphanesi sunar. Ancak P2 yalnızca geliştiriciler için değildir: oyun grupları, kulüpler, topluluklar ve verisini kendi sunucusunda tutmak isteyen tüm ekipler kullanabilir.

- Ürün sitesi: <https://p2.tdagaming.net>
- Dokümantasyon: <https://p2.tdagaming.net/docs/>
- Gizlilik politikası: <https://p2.tdagaming.net/docs/privacy/> (özet: [docs/PRIVACY.md](docs/PRIVACY.md))
- Kaynak depo: <https://git.tdagaming.net/captainTvZ/P2>
- Windows istemcisi: <https://p2.tdagaming.net/updates/client/P2-Client-Setup-1.0.2.exe>
- Windows sunucusu: <https://p2.tdagaming.net/downloads/P2-Server-Setup-1.0.2.exe>
- Ubuntu sunucusu: <https://p2.tdagaming.net/downloads/P2-Server-Ubuntu-24.04-x64-1.0.2.tar.gz>

P2 tamamen Rust ile yazılmıştır: masaüstü istemcisi ve sunucu konsolu GPU üzerinde çizilen yerli bir arayüz (GPUI) kullanır, tarayıcı motoru içermez; sunucu axum + sqlx + PostgreSQL üzerinde çalışır.

## Neler var?

- Proje ve topluluk kanalları, doğrudan mesajlar, grup sohbetleri, yanıtlar, alıntılar ve tepkiler
- Sesli görüşme, kamera, ekran paylaşımı, bildirimler ve oyun içi overlay
- Organizasyonlar, roller, kullanıcı grupları ve kanal bazlı izinler
- Kanban panoları, sprint'ler, görevler, notlar ve belgeler
- Proje build'leri için sürüm yönetimi, geri dönüş ve içerik tanımlı küçük yamalar
- Oyuncular için kurulum, otomatik güncelleme, arkadaş etkinliği ve oyun kütüphanesi
- PostgreSQL tabanlı self-hosted sunucu, `.p2backup` yedeği ve Windows/Linux servisleri

## Mimari

Depo kökü bir Cargo çalışma alanıdır (`Cargo.toml`). Ürün sürümünün tek kaynağı
`[workspace.package] version` alanıdır (şu an `1.0.2`).

| Yol | İçerik |
|---|---|
| `apps/p2-client` | Masaüstü istemcisi (Windows, GPUI): giriş/kurulum, çalışma alanı, sohbet, DM ve gruplar, aramalar, görev panosu, belgeler, kütüphane, yönetim paneli, ayarlar, tepsi, tek örnek, güncelleyici |
| `apps/p2-server` | API sunucusu (axum + sqlx): REST, socket.io `/stream` gerçek zamanlı olaylar, aramaların sinyalleşmesi, sürüm deposu, `/updates` güncelleme kanalları; CLI: `migrate`, `backup`, `restore`, `--p2-server` (servis kipi) |
| `apps/p2-server-console` | P2 Server Console (Windows): veritabanı ayarı, başlat/durdur, Windows servisi, yedek al/yükle, güncelleme |
| `apps/p2-overlay` | Oyun içi overlay host'u (oyun süreci başına): ekran dışı GPUI panel, bildirimler, HUD, araç pencereleri, ekran görüntüsü |
| `apps/p2-overlay-host`, `apps/p2-d3dtest` | Overlay geliştirme/test araçları (D3D11/D3D12 test "oyunu") |
| `apps/p2-callbot` | Test için arama katılımcısı (WebRTC, ses aygıtı kullanmaz) |
| `apps/p2-wincap` | Test aracı: bir sürecin penceresini PNG'ye alır, yalnız o pencereye mesaj gönderir |
| `apps/p2-showcase` | Arayüz bileşen vitrini ve ölçüm uygulaması |
| `crates/p2-client-core` | API istemcisi, oturum (DPAPI), yerel ayarlar, gerçek zamanlı bağlantı, eski Electron profilinden taşıma |
| `crates/p2-ui` | Tasarım sistemi: P2 Mavi token'ları, temalar, hareket, ortak bileşenler |
| `crates/p2-i18n` | 11 dil (`locales/*.json`) |
| `crates/p2-media` | Arama motoru: WebRTC örgü, mikrofon/hoparlör, ses işleme, bilgisayar sesi (WASAPI loopback), ekran/kamera yakalama |
| `crates/p2-patcher` | Kütüphane motoru: içerik tanımlı parçalama, kurulum/güncelleme/doğrulama/onarım, oyun başlatma, sürüm yükleme |
| `crates/p2-overlay-client`, `crates/p2-overlay-ipc` | İstemci tarafı overlay yöneticisi (enjeksiyon, süreç ağacı) ve DLL ile paylaşımlı bellek sözleşmesi |
| `crates/p2-server-ops` | Sunucu işletimi: `server.json`, günlük, göçler (`migrations/`, ikiliye gömülü), Windows servisi, `.p2backup` |
| `crates/p2-updater` | Güncelleyici: `latest.yml`, indirme, sha512 doğrulama, sessiz kurulum |
| `native/overlay` | Oyuna enjekte edilen C++ DLL + enjektör (CMake/MSVC), bkz. [native/overlay/README.md](native/overlay/README.md) |
| `packaging/` | Windows NSIS kurulumları, doğrulama, yayın, Linux sunucu paketi; `vendor/` derleme araçları (NSIS, WinSW, 7za) |
| `deploy/` | Linux (`install.sh`, systemd hizmeti, `p2-serverctl`, Electron dönemi kurulumdan yükseltme) ve Windows servis dosyaları |
| `tools/` | Uçtan uca testler (`slice*-test.mjs`, `faz8-test.mjs`, `server-ops/console-e2e.mjs`) ve PowerShell yardımcıları |
| `third_party/` | Yerel yamalı GPUI bağımlılıkları, bkz. [third_party/README.md](third_party/README.md) |
| `assets/` | Uygulama simgeleri (istemci ve sunucu) |

## Gereksinimler

- Rust stable (MSVC araç zinciriyle): `winget install Rustlang.Rustup`
- İstemci, sunucu konsolu ve kurulum paketleri için Windows 10/11 x64 + Visual Studio C++ derleme araçları (MSVC + Windows SDK; overlay DLL'i için CMake, VS ile gelir)
- Sunucu için PostgreSQL 15+ (sunucu Windows ve Linux'ta çalışır)
- Paketleme ve uçtan uca testler için Node.js 22+ (yalnız yerleşik modüller; `npm install` yok)

Derleme çıktısı OneDrive dışında, `C:\p2-rust-target` altında tutulur (`.cargo/config.toml`);
senkronizasyon `target/` klasörünü kilitleyip yavaşlatıyordu. Linux'ta `CARGO_TARGET_DIR=target` verin.

## Derleme, çalıştırma, test

```bash
cargo build --release                  # tüm çalışma alanı
cargo run --release -p p2-client       # istemci
cargo run --release -p p2-server-console

# Sunucu (P2 şemasındaki bir PostgreSQL; göçler: p2-server migrate)
DATABASE_URL=postgresql://... JWT_ACCESS_SECRET=... JWT_REFRESH_SECRET=... PORT=4000 \
  cargo run --release -p p2-server

# Birim testleri (ses aygıtı açılmaz)
P2_NO_AUDIO_DEVICE=1 cargo test --release --workspace
```

Veritabanı testleri (`crates/p2-server-ops/tests/db.rs`) ve uçtan uca testler bir test PostgreSQL'i
kullanır (`C:/p2-pg`, port 55432, ortam `C:/p2-pg/test-env.json`); yoksa veritabanı testleri atlanır.
Uçtan uca testler derlenmiş ikilileri kullanır ve `P2_API` verilmezse Rust sunucusunu kendileri başlatır:

```bash
node tools/slice2-test.mjs             # her dosyanın başında amacı ve önkoşulları yazar
```

İstemci tanılama: `P2_TRACE_SOCKET=1` (gerçek zamanlı bağlantı), `P2_TRACE_RENDER=1` (çizim),
`P2_PROFILE_DIR=<klasör>` (ayrı profil), `P2_NO_AUDIO_DEVICE=1` (hiçbir ses aygıtı açılmaz).

## Paketleme

Windows kurulumları (komutlar depo kökünden; araçlar `packaging/vendor` altında, npm gerekmez):

```bash
node packaging/build-installers.mjs                       # cargo build + istemci + sunucu kurulumları
node packaging/build-installers.mjs --product client --skip-cargo --version 1.0.2
#   → C:\p2-rust-target\dist\client\P2-Client-Setup-<v>.exe + latest.yml
#   → C:\p2-rust-target\dist\server\P2-Server-Setup-<v>.exe + latest.yml
node packaging/verify-installer.mjs                       # kurulumu ÇALIŞTIRMADAN açıp denetler
node packaging/publish.mjs --skip-build                   # P2_UPDATE_DIR'e (yoksa %ProgramData%\P2\updates) yayın
```

Overlay DLL'i ve enjektör `native/overlay/dist`'te yoksa `build-installers.mjs` onları CMake ile derler
(`node packaging/build-overlay.mjs`). Kurulumlar önceki sürümlerle aynı kimliği taşır ("P2 Client" /
"P2 Server", aynı kayıt anahtarları, klasör, kısayollar, AppUserModelID); kurulu sürümün üzerine
yerinde yükseltir.

Ubuntu sunucu paketi Linux'ta (ya da doğrudan sunucuda) derlenir:

```bash
CARGO_TARGET_DIR=target cargo build --release -p p2-server
node packaging/linux/build-linux-server.mjs --binary target/release/p2-server
#   → P2-Server-Ubuntu-24.04-x64-<v>.tar.gz (+ .sha256)
```

## Dağıtım

- **Ubuntu:** paketi açıp `sudo bash install.sh` (yeni kurulum ya da yükseltme). Node tabanlı eski
  bir kurulumdan geçiş: `deploy/linux/upgrade-from-electron.sh` (önce tam yedek, başarısızlıkta
  `rollback.sh` ile otomatik geri dönüş). Ayrıntılar: [docs/INSTALL-LINUX.md](docs/INSTALL-LINUX.md)
- **Windows:** `P2-Server-Setup-<v>.exe`, ardından Server Console. Ayrıntılar:
  [docs/INSTALL-WINDOWS.md](docs/INSTALL-WINDOWS.md)
- İstemciler güncellemeleri sunucunun `/updates/client/latest.yml` kanalından alır; sunucu konsolu
  `/updates/server/latest.yml` kanalından.

### Hızlı kurulum (Ubuntu)

```bash
tar -xzf P2-Server-Ubuntu-24.04-x64-1.0.2.tar.gz
cd P2-Server-Ubuntu-24.04-x64-1.0.2
sudo bash install.sh
sudo p2-serverctl status
```

## Profil ve veri konumları

| Konum | İçerik |
|---|---|
| `%APPDATA%\P2-Rust` | İstemci profili: ayarlar, oturum (DPAPI şifreli), kütüphane kaydı. İlk açılışta eski Electron profili (`%APPDATA%\P2`) varsa ayarlar, oturum ve kütüphane buraya taşınır; eski profil olduğu gibi kalır |
| `%ProgramData%\P2` | Windows sunucusu: `server.json`, günlükler, yüklenen dosyalar, sürüm deposu (`releases`), güncelleme klasörü (`updates`) |
| `/etc/p2-server/p2-server.env`, `/var/lib/p2-server` | Ubuntu sunucusu: ortam dosyası ve veri (yüklemeler, `releases`, `updates`) |

## Oyun kütüphanesi ve build dağıtımı

Yetkili kullanıcı bir build klasörü yükler; ekip kurar, çalıştırır ve canlı sürüm değiştiğinde yalnız değişen parçaları indirir. Dosyalar içerik tanımlı parçalama ile 1–4 MB bloklara ayrılır ve aynı içerik sunucuda yalnız bir kez tutulur.

## Güvenlik ve yedekleme

P2 Server'ı internete doğrudan `:4000` üzerinden açmak yerine HTTPS reverse proxy arkasında çalıştırın; PostgreSQL `5432` portunu internete açmayın. Üretim notları: [docs/SECURITY.md](docs/SECURITY.md).

Windows Server Console'daki **Yedek al** düğmesi ya da Ubuntu'da `sudo p2-serverctl backup` veritabanını ve yüklenen dosyaları tek `.p2backup` dosyasına toplar (`sudo p2-serverctl restore <dosya>` ile geri yüklenir). Oyun build'leri çok büyük olabileceği için yedeğe isteğe bağlı eklenir.

## Belgeler

- [docs/INSTALL-WINDOWS.md](docs/INSTALL-WINDOWS.md), [docs/INSTALL-LINUX.md](docs/INSTALL-LINUX.md), [docs/SECURITY.md](docs/SECURITY.md)
- [docs/patcher-plan.md](docs/patcher-plan.md) — kütüphane/patcher tasarımı
- [docs/rust-gecis-plani.md](docs/rust-gecis-plani.md) — Electron'dan Rust'a geçişin tarihçesi (tamamlandı)

## Lisans

P2 kaynak kodu tescillidir, tüm hakları saklıdır (`Cargo.toml`: `license = "UNLICENSED"`). Üçüncü taraf
bileşenler kendi lisanslarıyla gelir: `third_party/` (GPUI yamaları, Apache-2.0), `crates/p2-ui/assets/fonts`
(Inter, SIL OFL), `packaging/vendor/` (NSIS, WinSW, 7-Zip; lisans dosyaları klasörlerinde).
