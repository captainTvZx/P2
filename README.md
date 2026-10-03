# P2 0.7.20

## Ubuntu 24.04 LTS sunucu

`npm run dist:server:linux` komutu, Node.js çalışma zamanı dahil taşınabilir Ubuntu x64 paketini `release-package/linux` altında üretir.

```bash
tar -xzf P2-Server-Ubuntu-24.04-x64-0.7.20.tar.gz
cd P2-Server-Ubuntu-24.04-x64-0.7.20
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
3. Hepsi bu: P2 Client ve P2 Server kendi kanallarını ayrı ayrı denetler. Açık istemci yeni sürümü indirip sessizce kurar; Server Console sunucu güncellemesi hazır olduğunda uyarır, onayda API'yi durdurup kurulumu yapar ve yeni sürümü yeniden açar.
