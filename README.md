# P2 0.8.0

## Ubuntu 24.04 LTS sunucu

`npm run dist:server:linux` komutu, Node.js çalışma zamanı dahil taşınabilir Ubuntu x64 paketini `release-package/linux` altında üretir.

```bash
tar -xzf P2-Server-Ubuntu-24.04-x64-0.7.26.tar.gz
cd P2-Server-Ubuntu-24.04-x64-0.7.26
sudo bash install.sh
sudo p2-serverctl status
```

Kurulum `p2-server.service` systemd hizmetini kaydeder ve bilgisayar açılışında otomatik başlatır. Yönetim komutları:

```bash
sudo p2-serverctl logs
sudo p2-serverctl restart
sudo p2-serverctl backup
sudo p2-serverctl restore /yol/P2-Server-Backup.p2backup
```

Windows P2 Server Console içindeki **Yedek al** ve **Yedek yükle** düğmeleri de aynı `.p2backup` biçimini kullanır. Yedek, PostgreSQL veritabanını ve yüklenen dosyaları içerir.

React/Electron masaüstü istemcisi, NestJS API ve PostgreSQL tabanlı geliştirici oyun platformu.

## Komutlar

- `.env.example` dosyasını `.env` olarak kopyalayıp PostgreSQL bağlantısını ayarlayın
- `npm run db:generate` — Prisma istemcisini üretir
- `npm run db:migrate` — veritabanı migration'larını uygular
- `npm run dev` — web, API ve Electron geliştirme ortamını birlikte açar
- `npm run test:server` — backend testlerini çalıştırır
- `npm run build` — paketlenmiş uygulamayı üretir
- `npm run dist` — `P2 Client` ve `P2 Server` Windows kurulumlarını ayrı ayrı `release-package/client` ve `release-package/server` içine üretir
- `npm run dist:client` — yalnız istemci kurulumunu üretir
- `npm run dist:server` — yönetim konsollu API/güncelleme sunucusu kurulumunu üretir
- `npm run publish:update` — yeni sürümü paketleyip sunucunun güncelleme klasörüne (`%ProgramData%\P2\updates`) koyar; açık uygulamalar sürümü anlık duyuruyla, duyuru kaçırılırsa en geç 5 dakika içinde görür
- `npm run test:updates` — güncelleme ucunun testleri

## Kütüphane (oyun sürümleri)

Sol raydaki **Kütüphane**, projelerin build sürümlerini dağıtır: yetkili kişi bir build klasörü yükler, ekip indirip çalıştırır, **canlıya alınan** sürüme kurulu oyunlar kendiliğinden güncellenir.

- Steam düzeni: solda tüm projeler (indirme yüzdesi adın sağında), üstte kapak → yeşil **ÇALIŞTIR / DURDUR** şeridi (oynama süresi, son çalıştırma, gerekli alan, kurulu sürüm, son güncelleme, kategori), ortada ekibin elle yazdığı **Aktivite** akışı (mesaj editörüyle aynı: kalın/italik/bağlantı/emoji/görsel), sağda **Oynayan arkadaşlar**, **Başarımlar**, **İncelemeler** ve **Görevler**.
- Sürümler: yayınlanan her sürüm sağ tık → **Sürümler** menüsünden seçilebilir (elle seçilen sürüm "sabit" kalır, otomatik güncellenmez). Hangi sürümün herkese gideceğini yalnız `release.publish` yetkisi olanlar **Sürüm yönetimi → Canlıya al** ile belirler; kategorinin ilk yayını kendiliğinden canlı olur, eski bir yayını canlıya alarak geri dönülür.
- Yayınlanan her sürümün notları Kütüphane **Aktivite** akışına "Sürüm" rozetiyle kendiliğinden düşer; canlıya alma da kısa bir gönderi bırakır.
- **Oyun içi overlay (Shift+2)**: 0.9.1'den itibaren pakete dahil (`resources/overlay`: `p2overlay64.dll`, `p2inject64.exe`, `p2overlayhost.node`). D3D11 oyunlarda çalışır; D3D12/Vulkan/OpenGL ve anti-cheat'li build'lerde kendiliğinden kapalı kalır, oyun normal açılır. Ayarlar > Kütüphane'den kapatılabilir; sürüm bazında da yükleyici kapatabilir. Derleme: VS 2022+/2026 C++ araçları + CMake (VS ile gelen yeter) + Python 3 (node-gyp için); `npm run build:overlay`, `npm run build:overlay-host`, `npm run overlay:collect`.
- Çalıştırınca ekranın ortasında P2'den bağımsız küçük bir **başlatılıyor** kutusu çıkar (oyun penceresi gelince kapanır). Oyun açılınca overlay **sağ üstte** köşe bildirimi gösterir ("P2 overlay hazır · Shift+2"); Steam'in sağ altıyla çakışmaz. Overlay yoksa Windows bildirimi düşer.
- Masaüstü kısayolu projenin kendi ikonuyla oluşur (ikon yoksa oyunun exe simgesi).
- Çalıştırınca çevrimiçi ekibe Steam tarzı köşe bildirimi gider; Ayarlar > Kütüphane'den "Çalıştırdığımı ekibe duyur" ve "Ekipten biri çalıştırınca bildir" kapatılabilir.

- Parçalama **içerik tanımlı**dır (FastCDC benzeri Gear kayan özeti; 1–4 MB, ortalama 2 MB): dosyanın bir yerine bayt girince yalnız o civardaki parça değişir, Steam'deki gibi küçük yamalar iner. 0.9.2 öncesi sabit 4 MB dilimli sürümlerden yeni düzene ilk geçişte değişen dosyalar bir kez tam iner; sonrası küçüktür. Parametreler (`electron/cdc.cjs`) istemciler arasında asla değiştirilmemeli.
- Build dosyaları sunucuda içerik adresli en çok 4 MB'lık parçalar olarak `%ProgramData%\P2\releases` (Linux: `/var/lib/p2/releases`) altında saklanır; aynı parça tek kez tutulur, güncellemede yalnız değişen parçalar iner. Konum `P2_RELEASE_DIR` ile değiştirilebilir.
- Yetkiler: **Sürüm yükle ve yayınla** (`release.publish`), **Sürüm indir** (`release.download`, herkese açık başlar).
- Kullanıcı ilk indirmede bir disk seçer; oyunlar `<disk>\P2ProjectLibrary\<Proje>\` altına kurulur.
- P2 Server Console'daki **Yedek al**, kütüphane dosyalarını isteğe bağlı olarak (çok büyük olabilir) yedeğe dahil eden bir kutu sunar; varsayılan dışarıdadır.

## VDS ve güncelleme dağıtımı

P2 Client ayrı bir güncelleme servisine veya geliştiricinin bilgisayarına sabitlenmez. Kullanıcının bağlandığı P2 API adresi otomatik olarak güncelleme kaynağıdır. İstemci `https://p2.ornek.com/updates/client/latest.yml`, sunucu konsolu ise kendi API'sindeki `https://p2.ornek.com/updates/server/latest.yml` kanalını kullanır. Eski istemciler için `/updates/latest.yml` uyumluluk kopyası korunur.

- Windows VDS'de P2 Server Console içindeki **Windows servisi olarak kaydet** düğmesi yönetici izni ister ve `P2 Server (P2Server)` hizmetini otomatik başlangıçla `services.msc` içine kaydeder. Kayıtlıysa arayüz yalnız **Servisi kaldır** düğmesini gösterir.
- Konsoldaki **Veri klasörü** düğmesi `%ProgramData%\P2` dizinini açar. Client ve Server yayınları sırasıyla `updates\client` ve `updates\server` klasörlerindedir.
- Başka bir sunucu düzeninde aynı klasör `P2_UPDATE_DIR` ortam değişkeniyle değiştirilebilir.
- `npm run publish:update` komutu çalıştırıldığı makinenin yayın klasörüne yazar. VDS uzaktaysa `release-package/client` içeriği VDS'deki update klasörüne aktarılmalıdır.
- İstemciler yalnız bağlandıkları VDS'den indirir; yayıncı bilgisayarının `127.0.0.1` adresi uzaktaki kullanıcılara gönderilmez.

## Yeni sürüm yayınlama

1. `package.json` ve `server/package.json` içindeki `version` alanını yükselt.
2. Sunucuyu barındıran bilgisayarda `npm run publish:update` çalıştır.
3. Hepsi bu: P2 Client ve P2 Server kendi kanallarını ayrı ayrı denetler. Açık istemci yeni sürümü indirip sessizce kurar; Server Console da güncellemeyi indirir indirmez onay istemeden API'yi durdurur, sessizce kurar ve P2 Server'ı yeniden açar (kurulum başarısız olursa 5 dakikada bir yeniden dener).
