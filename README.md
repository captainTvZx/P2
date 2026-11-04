# P2

P2; ekipler, oyuncu toplulukları ve arkadaş grupları için self-hosted masaüstü iletişim ve çalışma platformudur. Kanallar, doğrudan/grup mesajları, sesli görüşmeler ve ekran paylaşımını; organizasyon, rol ve proje düzeniyle birleştirir.

Oyun geliştiren ekipler için ayrıca görev panoları, belgeler, build dağıtımı, içerik tanımlı güncellemeler ve ortak oyun kütüphanesi sunar. Ancak P2 yalnızca geliştiriciler için değildir: oyun grupları, kulüpler, topluluklar ve verisini kendi sunucusunda tutmak isteyen tüm ekipler kullanabilir.

- Ürün sitesi: <https://p2.tdagaming.net>
- Dokümantasyon: <https://p2.tdagaming.net/docs/>
- Kaynak depo: <https://git.tdagaming.net/captainTvZ/P2>
- Windows istemcisi: <https://p2.tdagaming.net/updates/client/P2-Client-Setup-0.9.9.exe>
- Windows sunucusu: <https://p2.tdagaming.net/downloads/P2-Server-Setup-0.9.9.exe>
- Ubuntu sunucusu: <https://p2.tdagaming.net/downloads/P2-Server-Ubuntu-24.04-x64-0.9.9.tar.gz>

## Neler var?

- Proje ve topluluk kanalları, doğrudan mesajlar, grup sohbetleri, yanıtlar, alıntılar ve tepkiler
- Sesli görüşme, ekran paylaşımı, bildirimler ve oyun içi overlay
- Organizasyonlar, roller, kullanıcı grupları ve kanal bazlı izinler
- Kanban panoları, sprint'ler, görevler, notlar ve belgeler
- Proje build'leri için sürüm yönetimi, geri dönüş ve içerik tanımlı küçük yamalar
- Oyuncular için kurulum, otomatik güncelleme, arkadaş etkinliği ve oyun kütüphanesi
- PostgreSQL tabanlı self-hosted sunucu, `.p2backup` yedeği ve Windows/Linux servisleri

## Hızlı kurulum

### Windows P2 Server

1. PostgreSQL 15+ kurun.
2. `P2-Server-Setup-0.9.9.exe` yükleyicisini çalıştırın.
3. Server Console'da PostgreSQL bağlantısını girin ve **Kaydet, tabloları hazırla ve başlat** düğmesine basın.
4. **Windows servisi olarak kaydet** ile otomatik başlangıcı etkinleştirin.

Ayrıntılı kılavuz: [docs/INSTALL-WINDOWS.md](docs/INSTALL-WINDOWS.md)

### Ubuntu P2 Server

```bash
tar -xzf P2-Server-Ubuntu-24.04-x64-0.9.9.tar.gz
cd P2-Server-Ubuntu-24.04-x64-0.9.9
sudo bash install.sh
sudo p2-serverctl status
```

Ayrıntılı kılavuz: [docs/INSTALL-LINUX.md](docs/INSTALL-LINUX.md)

## Geliştirme

```bash
npm install
cp .env.example .env
npm run db:generate
npm run db:migrate
npm run dev
```

Temel komutlar:

- `npm run test:server` — backend testleri
- `npm run test:ui` — istemci birim testleri
- `npm run build:web` — web istemcisi
- `npm run dist:client` — Windows istemci yükleyicisi
- `npm run dist:server` — Windows Server Console yükleyicisi
- `npm run dist:server:linux` — Ubuntu x64 taşınabilir sunucu paketi
- `npm run publish:update` — istemci ve sunucu güncelleme kanallarını yayınlama

## Oyun kütüphanesi ve build dağıtımı

Yetkili kullanıcı bir build klasörü yükler; ekip kurar, çalıştırır ve canlı sürüm değiştiğinde yalnız değişen parçaları indirir. Dosyalar FastCDC benzeri içerik tanımlı parçalama ile 1–4 MB bloklara ayrılır ve aynı içerik sunucuda yalnız bir kez tutulur.

- Windows veri dizini: `%ProgramData%\P2`
- Linux veri dizini: `/var/lib/p2-server`
- Linux sürüm deposu: `/var/lib/p2-server/releases`
- İstemci güncelleme kanalı: `/updates/client/latest.yml`
- Sunucu güncelleme kanalı: `/updates/server/latest.yml`

## Güvenlik

P2 Server'ı internete doğrudan `:4000` üzerinden açmak yerine HTTPS reverse proxy arkasında çalıştırın. PostgreSQL `5432` portunu internete açmayın. Üretim notları için [docs/SECURITY.md](docs/SECURITY.md) dosyasını okuyun.

## Yedekleme

Windows Server Console içindeki **Yedek al** düğmesi veya Ubuntu'da aşağıdaki komut veritabanı ve yüklenen dosyaları tek `.p2backup` dosyasına toplar:

```bash
sudo p2-serverctl backup
sudo p2-serverctl restore /yedekler/P2-Server-Backup.p2backup
```

Oyun build'leri çok büyük olabileceği için yedeğe isteğe bağlı eklenir.
