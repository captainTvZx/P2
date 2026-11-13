> **Geçiş tamamlandı (0.9.9):** Electron/React/NestJS kodu depodan kaldırıldı; Rust çalışma alanı depo kökünde. Bu belge tarihsel kayıttır (yollar `rust/…` olarak geçer).

# P2 — Rust ve GPU arayüzüne tam geçiş planı

Durum: **Faz 0 tamamlandı** — dört spike da geçti (arayüz, WebRTC, overlay, sunucu), sonuçlar §8'de;
kod `rust/` altında (`rust/faz-0` dalı). Hedef: Electron + React + NestJS yığınının
**tamamını** Rust'a taşımak. Bu kapsama istemci, sunucu, sunucu konsolu, Windows
servisi, güncelleyici, kütüphane/patcher ve oyun içi overlay dahil. Bütün sayfalar,
pencereler ve animasyonlar GPU üzerinde çizilen yerli bir Rust arayüzüyle yeniden
yazılacak. Tarayıcı motoru (Chromium/WebView) kullanılmayacak.

Hazırlayan: Claude (2026-11-12), mevcut 0.9.9 kod tabanının envanterine dayanır.

---

## 1. Neden ve başarı ölçütleri

0.9.9'un kurulu istemcisinde 2026-11-12'de yapılan ölçüm (giriş yapılmış, pencere
simge durumunda):

| Ölçüt | Electron 0.9.9 | Rust hedefi |
|---|---|---|
| Özel bellek (toplam) | ~390 MB, 5 süreç | **≤ 150 MB**, tek süreç (+ gerekirse medya yardımcısı) |
| GPU belleği (VRAM) | ~116 MB | ≤ 60 MB |
| Simge durumunda CPU | GPU süreci tek çekirdeğin %24'ü, arayüz %14'ü | **~%0**: görünmeyen pencere hiç çizilmez |
| Soğuk açılış | splash + birkaç saniye | ≤ 1 sn ilk kare |
| Kurulum boyutu | Chromium dahil ~100+ MB | ≤ 40 MB (WebRTC kütüphanesi dahil) |

Bu hedefler kesin değil. **Faz 0'daki prototiplerle doğrulanacak.** Ölçüt
karşılanmazsa plan Faz 0 sonunda yeniden değerlendirilir.

Sabit kurallar:
- Arayüz yalnızca **değişiklik olduğunda** çizilir. Sürekli kare üretimi sadece
  aktif bir animasyon veya video sırasında olur, pencere gizliyken asla.
- Kullanıcıya görünen davranış 0.9.9 ile **birebir aynı** olmalı (eşdeğerlik
  listesi: Ek A). Geçiş sırasında "şimdilik bu yok" diye yayın yapılmaz.
- Sunucu protokolü geçiş boyunca geriye uyumlu kalır. Eski Electron istemcisi ile
  yeni Rust istemcisi aynı sunucuda, aynı odada, aynı aramada birlikte çalışabilmeli.

---

## 2. Bugünkü sistemin envanteri (taşınacak her şey)

| Katman | İçerik | Büyüklük |
|---|---|---|
| Arayüz (React) | 60+ bileşen, overlay araç pencereleri, 9 dil | ~18.300 satır TS/TSX + ~5.300 satır CSS |
| Tasarım | P2 Mavi token'ları, açık/koyu tema, vurgu rengi seçimi | `theme.ts`, `p2-theme.css`, `ui.css`, `motion.css` |
| Animasyon | 42 keyframe, ~140 transition, elastik/pop easing'ler | `motion.css` token'ları (`--m-*`, `--ease-*`) |
| Sunucu (NestJS) | **173 REST ucu**, 16 socket.io olayı, JWT + 2FA | ~5.800 satır |
| Veritabanı | PostgreSQL, Prisma: **50 model/enum**, 50 migration | `server/prisma/schema.prisma` |
| Electron ana süreç | Pencereler, tepsi, bildirim, güncelleyici, oturum saklama, ~80 IPC kanalı | ~4.000 satır |
| Kütüphane/patcher | İçerik tanımlı parçalama (CDC/Gear), delta indirme, yükleme, oyun başlatma | `patcher.cjs`, `cdc.cjs`, `patcherManifest.cjs` |
| Sunucu uygulaması | Sunucu konsolu penceresi, Windows servisi, Linux paketi | `server-main.cjs`, `service/`, `scripts/build-linux-server.mjs` |
| Native | Oyun içi overlay DLL'i (D3D11/D3D12 hook, girdi hook, ekran görüntüsü), enjektör, sistem sesi loopback | ~1.700 satır C++ |
| Medya | WebRTC **P2P mesh**, STUN/TURN, Opus öncelikli, ekran paylaşımı, kalite seviyeleri, PTT, ses ölçer | `CallProvider.tsx`, `media.ts`, `StreamPlayer.tsx` |

### 2.1 Sayfalar ve pencereler
- **Giriş ve kurulum:** giriş, kayıt, 2FA, sunucu kurulumu/bağlanma, `ServerProblem`, şifre gücü.
- **Çalışma alanı:** proje rayı, kanal listesi/kategoriler, kanal tarayıcısı, kanal içeriği (mesajlar, zengin metin, ekler, tepkiler, düzenleme geçmişi, yazıyor göstergesi, bahsetme, emoji önerileri), zengin yazı alanı, mesaj menüsü.
- **Direkt mesaj ve gruplar:** DM'ler, grup sohbeti, grup oluşturma, konuşma menüsü.
- **Profil ve kişiler:** profil paneli, varlık durumu (çevrimiçi/uzakta/meşgul), arkadaşlık.
- **Ayarlar:** ayarlar penceresi (cihazlar, ses, görünüm, kısayollar, güncelleme, sunucu), görünüm ayarları.
- **Yönetim:** yönetim paneli, ekip, erişim, roller, kanal izinleri, kullanıcı grupları, organizasyonlar ve davetler.
- **Araçlar:** görev panosu (pano, sprint, kanban, yorumlar), belgeler (**tuval**: düğüm/tel/şekil/görsel ve **sayfa** belgeleri, canlı imleçler, klasörler, izinler), proje notları, ekran görüntüleri paneli, etkinlik paneli, proje merkezi (gönderi, başarım, inceleme).
- **Kütüphane:** oyun listesi, indirme/kurulum/doğrulama/başlatma, sürüm yükleyici, sürüm farkı.
- **Arama:** gelen arama, arama kaplaması, yayın sahnesi ve oynatıcı, kalite menüsü, ekran paylaşım seçici, **arama widget'ı** (ayrı pencere), **ekran üstü çizim kaplaması**.
- **Diğer:** ek görüntüleyici (görsel, PDF, DOCX, arşiv), bildirimler/toast'lar, güncelleme bandı, açılış ekranı, açılır pencereler, UI vitrini (geliştirme).
- **Oyun içi overlay:** pencere yöneticisi ve araçlar (Sesli, Görevler, Notlar, Yakalama, Kütüphane, Genel Bakış, Performans, Sohbet, Ekip, Tarayıcı).

---

## 3. Hedef mimari

```
p2/                          (cargo workspace)
├─ crates/
│  ├─ p2-protocol      DTO'lar, socket olayları, hata kodları (serde) — istemci+sunucu ortak
│  ├─ p2-core          iş kuralları: izinler, sayfalama, ekran görüntüsü/not kuralları, sürüm farkı
│  ├─ p2-cdc           Gear/FastCDC parçalayıcı — cdc.cjs ile BAYT BAYT aynı kesimler
│  ├─ p2-ui            tasarım sistemi: token'lar, tema, bileşenler, hareket (motion) motoru
│  ├─ p2-i18n          9 dil (Fluent .ftl), derleme zamanı anahtar denetimi
│  ├─ p2-media         WebRTC, ses (yakalama/çalma/AEC/gürültü), ekran yakalama, video çözme→GPU doku
│  ├─ p2-client-core   API istemcisi, gerçek zamanlı bağlantı, önbellek/durum deposu, oturum
│  ├─ p2-patcher       kütüphane motoru: indirme, kurulum, doğrulama, yükleme, başlatma
│  └─ p2-overlay-ipc   overlay paylaşımlı bellek/doku protokolü (p2overlay_ipc.h'nin Rust karşılığı)
├─ apps/
│  ├─ p2-client        masaüstü uygulaması (tek exe)
│  ├─ p2-server        Axum API + gerçek zamanlı sunucu (Windows servisi / Linux systemd)
│  ├─ p2-server-console sunucu konsolu arayüzü
│  ├─ p2-overlay-dll   oyun içi hook DLL'i (Faz 7'de C++'tan Rust'a)
│  └─ p2-inject        enjektör
└─ tools/              veri/ayar taşıma, sözleşme testleri, i18n dönüştürücü
```

### 3.1 Arayüz çatısı (en kritik karar)

| Aday | Artı | Eksi |
|---|---|---|
| **GPUI** + `gpui-component` (önerilen) | Zed editörünü çizen motor; Windows'ta DirectX ile GPU çizimi, DirectWrite ile metin, güçlü metin/IME desteği, hazır bileşen kütüphanesi, yerleşik animasyon API'si, büyük listeleri sanallaştırma | API hâlâ hızlı değişiyor, belgeler az; ekran dışı (offscreen) çizim desteği belirsiz |
| **Iced** (yedek) | wgpu tabanlı, olgun, COSMIC masaüstü gibi büyük projelerde kullanılıyor, herhangi bir dokuya çizebilir | Zengin metin düzenleme ve karmaşık animasyon daha çok elle iş |
| Slint / egui / Makepad | Belirli alanlarda güçlü | Discord ölçeğinde zengin metinli bir uygulama için zayıf ya da lisans/olgunluk riski |

**Öneri:** GPUI. Son karar Faz 0'daki vitrin prototipiyle verilir (bkz. 4.0).
Seçilen çatı bir `p2-ui` katmanının arkasına alınır. Böylece sayfalar çatının
iç ayrıntılarına doğrudan bağlanmaz.

### 3.2 Sunucu yığını
- **HTTP:** `axum` + `tokio` + `tower` (sıkıştırma, CORS, hız sınırı, istek günlüğü).
- **Veritabanı:** `sqlx` (PostgreSQL, derleme zamanı sorgu denetimi). Mevcut şema
  **aynen** korunur. Prisma migration'ları dondurulup tek bir temel SQL'e
  çevrilir, sonraki değişiklikler `sqlx migrate` ile yapılır.
- **Gerçek zamanlı:** geçiş boyunca `socketioxide` ile **socket.io uyumlu** kalır
  (eski istemciler çalışmaya devam eder). Electron emekliye ayrıldıktan sonra
  isteğe bağlı olarak "protokol v2"ye geçilir: düz WebSocket + `p2-protocol` tipli mesajlar.
- **Ölçek (on binlerce kullanıcı):** PostgreSQL bu iş için doğru seçim: veri ilişkisel
  (kuruluş, üyelik, rol, izin, görev) ve on binlerce kullanıcı tek bir iyi ayarlanmış
  örneğin rahat taşıyacağı yük. Büyüyecek tablo mesajlardır: doğru indeksler, zaman bazlı
  bölümleme ve "son kayıttan devam" (keyset) sayfalaması. Asıl ölçek darboğazı gerçek
  zamanlı katmandır: bugün çevrimiçi durum ve aramalar tek sürecin belleğinde
  (`realtime.ts`). Rust sunucusu birden fazla kopyayla çalışabilsin diye bu durum
  paylaşılan bir katmana (socketioxide Redis/NATS adaptörü) taşınır; yüklenen dosyalar
  büyüdükçe diskten nesne depolamaya (S3/MinIO) geçer.
- **Kimlik:** `argon2` (mevcut PHC biçimli hash'ler aynen doğrulanır), `jsonwebtoken`,
  `totp-rs` (2FA). Bunun anlamı: **kimse şifresini yeniden belirlemek zorunda kalmaz.**
- **Dosyalar:** yüklemeler ve sürüm parçaları aynı klasör düzeninde kalır
  (`%ProgramData%\P2\uploads`, `releases`, `/var/lib/p2/...`).
- **Servis:** `windows-service` (Windows servisi) ve systemd birimi (Linux).

### 3.3 İstemci yerli katmanı (Electron'un yerine geçenler)

| Electron özelliği | Rust karşılığı |
|---|---|
| Çerçevesiz pencere, özel başlık çubuğu, sürükleme, büyütme | GPUI pencere API'si + `windows` crate (snap düzenleri için WM_NCHITTEST) |
| Tepsi simgesi ve menüsü | `tray-icon` |
| Windows bildirimleri (tıklanınca pencereye dönme) | WinRT toast (`windows` crate), AppUserModelID = `com.p2.desktop` korunur |
| Tek örnek, `p2://` protokolü, `--p2-play=` | Adlandırılmış mutex + adlandırılmış kanal; kayıt defteri girdisi aynen |
| `safeStorage` (oturum şifreleme) | DPAPI (`windows` crate) ya da `keyring` |
| Global kısayollar, PTT | `global-hotkey` + ham girdi (oyundayken PTT için düşük seviye kanca) |
| Ekran/pencere seçici (`desktopCapturer`) | Windows Graphics Capture (`windows-capture`) ile canlı küçük resimler |
| Sistem sesi loopback (node-gyp modülü) | WASAPI loopback (`windows` crate), `p2-media` içinde |
| Dosya diyalogları, klasörü aç, kısayol oluştur | `rfd`, `ShellExecute`, `IShellLink` |
| `electron-updater` + açılış güncelleme penceresi | Kendi güncelleyicimiz (bkz. Faz 8) |
| Açılır pencereler (popout), arama widget'ı, çizim kaplaması | Aynı süreçte ek GPUI pencereleri; şeffaf ve tıklamayı geçiren katman pencere |

---

## 4. Fazlar

Süreler, 3 kişilik bir ekip (1 arayüz, 1 sistem/medya, 1 sunucu/altyapı) için
**kaba tahminlerdir.** Tek geliştirici + Claude ile her faz yaklaşık 1,5–2 katına çıkar.

### Faz 0 — Teknik keşif ve karar (3–4 hafta)
Amaç, en riskli dört konuyu kod yazarak kanıtlamak. Ana geliştirmeye ancak
bunlar geçerse başlanır.

1. **Arayüz vitrini (GPUI, gerekirse Iced):**
   - 10.000 mesajlık sanallaştırılmış sohbet listesi.
   - Zengin metin: kalın, italik, kod bloğu, bahsetme, renkli emoji.
   - Türkçe, Japonca ve Çince **IME** ile yazma.
   - Elastik açılan bir modal, toast kuyruğu ve sayfa geçişi (`p2-pop-elastic`, `p2-page` birebir).
   - Açık/koyu tema geçişi.
   - **Ölçüm:** bellek, simge durumunda CPU, açılış süresi. Bölüm 1'deki tabloya yazılır.
2. **WebRTC uyumluluğu:** `p2-media` prototipi, **Electron 0.9.9 istemcisiyle aynı
   aramaya** girer. Ses (AEC/gürültü bastırma açık) ve ekran paylaşımı iki yönde de
   çalışmalı. Aday: Google libwebrtc'yi saran `libwebrtc` crate'i (LiveKit Rust SDK
   ile gelir). Saf Rust `webrtc-rs` yankı giderme ve video kodlayıcı içermediği için
   sadece yedek seçenek.
3. **Overlay çizimi:** Rust host, overlay arayüzünü ekran dışında bir dokuya çizip
   **mevcut C++ DLL'in** paylaşımlı bellek protokolüne (`p2overlay_ipc.h`) besler.
   Bir D3D11 ve bir D3D12 oyununda görünmeli.
4. **Sunucu iskeleti:** Axum + sqlx mevcut veritabanına bağlanır. `auth` uçları
   (giriş, yenileme, 2FA) ve `subscribe`/`presence` olayları çalışır. **Mevcut
   Electron istemcisi bu Rust sunucusuna giriş yapabilmeli.**

**Çıkış ölçütü:** dördü de çalışıyor ve ölçümler hedefe yakın. Değilse çatı
değiştirilir (Iced) ya da plan revize edilir.

### Faz 1 — Temeller (4–6 hafta)
- Cargo workspace, CI (derleme, `clippy`, testler, imzalı sürüm), hata/günlük altyapısı (`tracing`).
- **`p2-protocol`:** 173 ucun istek/yanıt tipleri ve 16 socket olayının yükleri.
  Mevcut NestJS DTO'larından çıkarılır. Yanlış alan adını derleme zamanında yakalamak için tek kaynak burası.
- **`p2-ui` tasarım sistemi:**
  - P2 Mavi token'ları (renk, boşluk, yarıçap, gölge, bulanıklık), açık/koyu tema, vurgu tonu.
  - Temel bileşenler: düğme, girdi, seçim kutusu, anahtar, kaydırıcı, sekme, menü, bağlam menüsü, modal, toast, ipucu, avatar, rozet, ilerleme çubuğu, sanal liste, sürükle-bırak, yeniden boyutlandırıcı.
- **Hareket motoru (`p2-ui::motion`):**
  - `motion.css`'teki süre ve easing token'ları birebir sabitlere dönüşür (`M_FAST=140ms`, `EASE_POP=cubic(.2,1.4,.4,1)`...).
  - 42 keyframe tek tek Rust animasyon tanımlarına çevrilir (fade, rise, page, slide, pop-elastic, scale-in, pulse, spin, bar-sweep...). Ek A'da her birinin karşılığı işaretlenir.
  - Kurallar: animasyon yalnız ekranda görünen öğede çalışır. Pencere gizliyse ya da sistemde "hareketi azalt" açıksa durur. Sonsuz animasyonlar (çevrimiçi nabzı, yazıyor noktaları) pencere arka plandayken donar.
  - 0.9.9'da ölçülen arka plan israfı bu kurallarla tasarım düzeyinde engellenir.
- **`p2-i18n`:** `src/locales/*.ts` dosyaları bir betikle Fluent (`.ftl`) dosyalarına çevrilir. Eksik anahtar derleme hatası verir.

### Faz 2 — Sunucu Rust'a (8–10 hafta, istemciyle paralel)
- Modül modül taşınır: `auth`, `preferences`, `organizations`, `projects`, `roles`/`permissions`/`channelPermissions`/`userGroups`, `direct`, `groups`, `projectHub`, `activity`, `notes`, `screenshots`, `tasks`, `documents`, `uploads`, `releases`/`releaseStore`/`releaseManifest`, `updates`, `admin`.
- `realtime.ts` (733 satır): oda yönetimi, varlık durumu, yazıyor göstergesi, belge imleçleri, **arama sinyalleşmesi** (`call:start/invite/accept/decline/leave/sync/media/moderate/signal`).
- **Sözleşme testleri:** her uç için NestJS'in gerçek yanıtları kaydedilir, aynı istek Rust sunucusuna atılır ve yanıtlar karşılaştırılır (alan alan, sıralama dahil).
- Mevcut Vitest testleri (`*.spec.ts`) Rust birim testlerine çevrilir.
- **Yan yana çalıştırma:** Rust sunucusu ayrı portta, aynı veritabanıyla bir süre gölge modda çalışır. Sonra port devralınır. Geri dönüş yolu: eski servisi yeniden başlatmak.
- Sunucu konsolu (GPUI), Windows servisi, Linux paketi ve sunucu güncelleyicisi (`server-updater`).

### Faz 3 — İstemci çekirdeği (10–12 hafta)
- Uygulama kabuğu: başlık çubuğu, tepsi, tek örnek, protokol, bildirimler, oturum saklama, açılış ekranı, popout pencereler, kapatma davranışı (sor/tepsiye/çık).
- `p2-client-core`: API istemcisi (`reqwest`), socket.io istemcisi, yeniden bağlanma, önbellek ve olay tabanlı durum deposu.
- Giriş, kayıt, 2FA, sunucu kurulumu/bağlanma, `ServerProblem`.
- Proje rayı, kanallar, kategoriler, kanal tarayıcısı, okunmamış sayaçları.
- **Sohbet:**
  - Markdown → GPUI metni (`pulldown-cmark`), kod renklendirme (`syntect`), bahsetme, bağlantı önizlemesi.
  - Tepkiler, düzenleme ve geçmişi, yazıyor göstergesi, emoji paneli ve öneriler (emoji verisi + renkli glif; DirectWrite Segoe UI Emoji ya da Twemoji atlası).
  - Dosya bırakma, ek yükleme, `RichComposer`.
- DM'ler, gruplar, profil paneli, varlık durumu, arkadaşlık.
- Ayarlar (görünüm, kısayollar, bildirim, güncelleme, sunucu), yönetim panelleri, roller ve izinler.
- **Ek görüntüleyici:**
  - Görseller: `image` crate (GIF/WebP animasyonları dahil), yakınlaştırma ve kaydırma GPU'da.
  - PDF: `pdfium-render` (sayfalar dokuya çizilir).
  - Arşivler: `zip`, `sevenz-rust`, RAR için `unrar`. Bugünkü `libarchive.js` kapsamıyla karşılaştırılacak.
  - DOCX: bkz. Karar 3.

### Faz 4 — Arama ve medya (8–10 hafta)
- `p2-media`:
  - Peer bağlantıları (mesh), STUN/TURN, `forceRelay`, Opus öncelik sırası, ses bit hızı.
  - Ekran paylaşımı: Windows Graphics Capture → donanım kodlayıcı. Kalite seviyeleri (`maxBitrate`, `scaleResolutionDownBy`, fps).
  - Ses: WASAPI ile yakalama ve çalma, yankı/gürültü/kazanç işleme (libwebrtc APM), cihaz seçimi ve sıcak takma, PTT, ses ölçer.
  - Sistem sesi paylaşımı (WASAPI loopback), arama istatistikleri.
- Gelen video kareleri doğrudan GPU dokusuna aktarılır, CPU kopyası yapılmaz (`StreamPlayer`, yayın sahnesi, tam ekran).
- Gelen arama, arama kaplaması, kalite menüsü, ekran paylaşım seçici, **arama widget'ı**, **ekran üstü çizim kaplaması** (şeffaf, tıklamayı geçiren pencere, kalem imleçleri).
- Uyumluluk testi: Rust ↔ Electron karışık aramalar (2, 4 ve 8 kişi).

### Faz 5 — Araçlar (8–10 hafta)
- **Görev panosu:** panolar, sprintler, kanban sürükle-bırak, görev ayrıntısı, yorumlar ve tepkileri, görev merkezi.
- **Belgeler:**
  - **Tuval:** düğümler, teller, şekiller, görseller, metin, kaydırma/yakınlaştırma. GPU için doğal bir iş; bugünkünden daha akıcı olması beklenir.
  - **Sayfa belgeleri:** zengin metin editörü, canlı imleçler, klasörler, izinler. **Planın en büyük tek iş kalemi** (bkz. Risk 2).
  - Bugün HTML olarak saklanan içerik, yeni editörün belge modeline **sunucu tarafında bir kerelik dönüştürmeyle** taşınır. Dönüştürme geri alınabilir: eski sütun silinmez.
- Proje notları, ekran görüntüleri paneli, etkinlik paneli, proje merkezi (gönderi, başarım, inceleme).

### Faz 6 — Kütüphane ve patcher (5–6 hafta)
- `p2-cdc`: Gear tablosu ve MIN/AVG/MAX/MASK sabitleri **birebir** taşınır. Altın test vektörleri JS uygulamasından üretilir. Tek bir kesim noktası bile farklıysa tekilleştirme bozulur, bu yüzden CI'da zorunlu test.
- İndirme/kurulum/doğrulama/onarma/kaldırma, duraklat/sürdür, eşzamanlılık (4 paralel), hız sınırı, kanal seçimi, başlatma argümanları, kısayol oluşturma.
- Sürüm yükleyici (sürdürülebilir yükleme), sürüm farkı görünümü, `p2://play` ile oyun başlatma.
- **Mevcut kurulumlar korunur:** oyunların kurulu olduğu klasörler ve `.p2` meta verisi yeni istemcide aynen tanınır, yeniden indirme olmaz.

### Faz 7 — Oyun içi overlay (6–8 hafta)
- **7a:** Overlay arayüzü (pencere yöneticisi, sabitleme, düzen kaydı, 10 araç penceresi, overlay bildirimleri) `p2-ui` ile yeniden yazılır. Ekran dışında çizilip mevcut C++ DLL'e beslenir (Faz 0'da kanıtlanan yol).
- **7b:** Paylaşımlı **CPU belleği yerine paylaşımlı GPU dokusu** (DXGI shared handle). Kare kopyası ortadan kalkar, oyundaki FPS kaybı azalır.
- **7c:** DLL ve enjektör Rust'a taşınır (`windows` crate, `retour` ile hook'lar; D3D11/D3D12 arka uçları, girdi hook'ları, ekran görüntüsü). Bu adım en sona bırakılır, çünkü C++ DLL zaten küçük (~1.700 satır) ve çalışıyor.
- Hile önleyici (anti-cheat) yazılımlarla uyumluluk listesi bugünkü haliyle korunur ve yeniden test edilir.

### Faz 8 — Dağıtım, güncelleme ve taşıma (4–5 hafta)
- **Kurulum:** NSIS ya da MSI (`cargo-packager` veya WiX). İstemci ve sunucu ayrı paketler (bugünkü gibi).
- **Güncelleyici:** bugünkü akış korunur (takım sunucusundan `latest.yml` benzeri bildirim, açılışta denetim, kapanışta sessiz kurulum). İkili fark (delta) güncellemesi `p2-cdc` ile yapılır, böylece güncellemeler küçülür.
- **Electron'dan taşıma** (son Electron sürümü 0.10.x bir "taşıma köprüsü" olur):
  - Ayarlar ve görünüm tercihleri: `localStorage` Chromium'un LevelDB dosyalarında duruyor. Bu yüzden son Electron sürümü bunları açılışta `%APPDATA%\P2\migrate.json` dosyasına yazar, Rust istemcisi ilk açılışta okur.
  - Oturum: DPAPI ile şifreli oturum yeni istemciye devredilir, **kullanıcı yeniden giriş yapmaz.**
  - Kütüphane kök klasörü, kurulu oyunlar, kısayollar, `p2://` kaydı, AppUserModelID (görev çubuğu sabitlemesi bozulmasın).
- Electron sürümünden Rust sürümüne güncelleme, mevcut güncelleyici üzerinden **tek tıkla** yapılır.

### Faz 9 — Beta, eşdeğerlik ve Electron'un emekliye ayrılması (4–6 hafta)
- İç ekip, "beta" güncelleme kanalında Rust istemcisini kullanır. Electron "stable" kanalında kalır.
- Ek A'daki eşdeğerlik listesinin her satırı işaretlenir. Performans tablosu (Bölüm 1) yeniden ölçülür.
- Hata/çökme raporlama (`tracing` + minidump).
- Kabul sonrası Rust stable olur. Electron kodu, NestJS, Prisma, Vite ve React depodan kaldırılır.
- İsteğe bağlı sonraki adım: protokol v2 (socket.io'dan düz WebSocket'e).

### Toplam süre (kaba)

| Faz | Süre | Paralellik |
|---|---|---|
| 0 Keşif | 3–4 hf | — |
| 1 Temeller | 4–6 hf | — |
| 2 Sunucu | 8–10 hf | 3–7 ile paralel |
| 3 İstemci çekirdeği | 10–12 hf | |
| 4 Medya | 8–10 hf | 3 ile kısmen paralel |
| 5 Araçlar | 8–10 hf | |
| 6 Kütüphane | 5–6 hf | 5 ile paralel |
| 7 Overlay | 6–8 hf | 5–6 ile paralel |
| 8 Dağıtım | 4–5 hf | |
| 9 Beta | 4–6 hf | |

3 kişiyle takvim olarak **~9–12 ay**, toplam iş **~25–32 kişi-ay**. Tek geliştirici
+ Claude ile 15–20 ay gerçekçi bir aralık.

---

## 5. Geçiş sırasında Electron sürümü

- Electron 0.9.x **bakım moduna** alınır: sadece hata düzeltmeleri ve güvenlik.
  Yeni özellikler doğrudan Rust tarafında yapılır. Aksi halde her özellik iki kez yazılır.
- Faz 0'dan önce, Electron'a ölçümde bulunan **arka plan çizim israfı** için küçük
  bir düzeltme yapılması önerilir (duruma göre `setBackgroundThrottling`, gizliyken
  animasyonları duraklatma). Geçiş yaklaşık bir yıl süreceği için kullanıcılar bu
  süre boyunca fan ve pil sorunu yaşamaz.

---

## 6. Riskler

| # | Risk | Etki | Önlem |
|---|---|---|---|
| 1 | GPUI API'si değişkenliği, belgelerin azlığı | Yavaşlama, yükseltme maliyeti | Sürüm sabitleme; `p2-ui` soyutlama katmanı; Iced yedeği Faz 0'da denenir |
| 2 | Zengin metin / belge editörü | En büyük iş kalemi | Faz 5'e geniş süre; önce Markdown tabanlı sade model; işbirliği için CRDT (`yrs`) değerlendirmesi |
| 3 | WebRTC (libwebrtc) derlemesi ve boyutu | Kurulum +10–20 MB, karmaşık derleme | Hazır derlenmiş ikililer; Faz 0'da Electron ile birlikte çalışma kanıtı |
| 4 | Web içeriği (YouTube gömmeleri, overlay "Tarayıcı" aracı) | Saf Rust ile mümkün değil | Karar 2 |
| 5 | DOCX önizleme kalitesi | `mammoth` kadar iyi Rust karşılığı yok | Karar 3 |
| 6 | Erişilebilirlik (ekran okuyucu) | Chromium'un hazır verdiği şey kaybolur | AccessKit entegrasyonu; Faz 9 kabulünde denetim |
| 7 | Mesh arama ölçeği | 6+ kişide bant genişliği (bugün de var) | Kapsam dışı; protokol v2 ile isteğe bağlı SFU |
| 8 | CDC uyumsuzluğu | Tüm kurulumların yeniden inmesi | Altın vektör testleri CI'da zorunlu |
| 9 | Bir yıllık çift bakım | Yorgunluk, kayma | Electron bakım modunda; fazlar arası kullanılabilir ara sürümler |

---

## 7. Senin vereceğin kararlar

1. **Arayüz çatısı:** GPUI (önerilen) mi, Iced mi? Faz 0 sonunda ölçümle kesinleşir.
2. **Web içeriği:** "hiç tarayıcı yok" kuralı YouTube gömmeleri ve overlay'deki
   Tarayıcı aracı için de geçerli mi?
   - (a) Varsayılan tarayıcıda aç. Tamamen Rust, ama özellik kaybı var.
   - (b) **Sadece bu iki yerde** sistemin WebView2'si `wry` ile kullanılır. Sadece açıkken bellek tüketir.
3. **DOCX önizleme:** basitleştirilmiş görünüm (metin, başlıklar, tablolar, görseller) yeterli mi, yoksa tam sayfa düzeni şart mı?
4. **Platformlar:** istemci sadece Windows mu? Rust ile Linux/macOS istemcisi göreceli olarak ucuz, ama overlay ve ekran yakalama platforma özel.
5. **Ekip ve takvim:** kaç kişi çalışacak? Bölüm 4'teki süreler buna göre güncellenir.
6. **Çevrimiçi noktası animasyonu:** Faz 0 ölçümüne göre sonsuz nabız etkin pencerede
   tek çekirdeğin ~%7'sini harcıyor. Öneri: nokta sabit olsun (yalnız durum değişince kısa geçiş).

---

## 8. Faz 0 sonuçları

Ölçüm makinesi: Windows 11, RTX 4060 Laptop + Iris Xe, ekran 2560×1440 @ 143 Hz.
Yöntem: Electron için kurulu 0.9.9 istemcisi; Rust için `--release` derlemeleri.

### Spike 1 — GPUI arayüz vitrini (`rust/apps/p2-showcase`) ✅

Kapsam: P2 düzeni (ray, kanallar, kişiler), 10.000 mesajlık sanal liste, zengin metin
(kalın/italik/kod/bahsetme/kod bloğu), renkli emoji, TR/JA/ZH karakterler, yazı alanı,
elastik modal, toast kuyruğu, kanal değişiminde sayfa geçişi, açık/koyu tema.
Çatı: GPUI (`gpui-kit 0.9.1` = gpui-pre 0.3.12 + gpui-component).

| | Electron 0.9.9 | Rust vitrin | Hedef (§1) |
|---|---|---|---|
| Özel bellek | ~390 MB, 5 süreç | **~97 MB, tek süreç** | ≤ 150 MB ✅ |
| VRAM | ~116 MB | **30 MB** | ≤ 60 MB ✅ |
| Simge durumunda CPU (tek çekirdek) | GPU %24 + arayüz %14 | **%0,3** | ~%0 ✅ |
| Görünür, boşta CPU | — | %1,5–3,75 | — (bkz. bulgu 4) |
| İlk kare | birkaç sn (splash) | 0,3–0,7 sn sıcak, ~4 sn soğuk | ≤ 1 sn ✅ (sıcak) |
| Exe | Chromium dahil 100+ MB | **21,5 MB** | ≤ 40 MB ✅ |

Bulgular:
1. **GPUI'de kutu ölçekleme yok** (dönüşüm yalnız SVG'de). `p2-pop-elastic`,
   `p2-scale-in` gibi büyüyerek girişler ölçek yerine kayma + elastik eğriyle yaklaşık
   yapıldı. Easing eğrileri birebir taşındı (`p2-ui::motion`, testli).
2. **`Animation::with_max_fps` gizli pencerede de çalışıyor**: kare döngüsünden bağımsız
   zamanlayıcıyla görünümü yeniden kurduruyor. Kullanılmıyor; sınırsız animasyon kare
   döngüsüne bağlı ve pencere gizlenince kendiliğinden duruyor.
3. **GPUI Windows katmanı küçültülmüş pencereyi de her ekran tazelemesinde boyatıyordu**
   (tek çekirdeğin ~%2,5'i). Yerel yamayla düzeltildi (`rust/third_party`, üst projeye
   önerilecek): simge durumunda %3 → %0,3.
4. **Görünür ve boşta %1,5–3,75 kalıyor**: GPUI her tazelemede kare isteğini işliyor.
   Ancak GPUI tarafında (iş yokken vsync döngüsünü uyutmak) çözülür; Faz 1'de üst projeye
   önerilecek ya da yamaya eklenecek.
5. **Mimari kural:** GPUI bildirim alan görünümü ve önbelleksiz bütün atalarını yeniden
   kurar. Ağır bölmeler ayrı varlık + `cached()`; sürekli animasyonlar küçük ayrı
   görünümlerde. Böylece nabız animasyonu sohbet listesini yeniden kurmuyor (%10,5 → %7).
6. 22 açılışta bir kez açıklanamayan kapanma görüldü, tekrarlanamadı (izleniyor).
7. **Doğrulanmadı:** IME ile gerçek yazma ve etkileşimlerin (toast/modal/tema) ekran
   doğrulaması elle yapılmalı. Otomatik giriş göndermek kullanıcının masaüstünü
   etkilediği için kullanılmayacak; Faz 1'de GPUI test kancalarıyla otomatikleştirilecek.

### Spike 4 — Rust sunucu iskeleti (`rust/apps/p2-server`) ✅

Kapsam: `/health`, `/setup/*`, `/auth/*` (kayıt, giriş, yenileme, çıkış, 2FA kurulum/açma/
kapama, profil, me) ve socket.io `/stream` (bağlantı doğrulama, `subscribe`,
`presence:prefer|idle|snapshot`, `presence:update|self` yayınları). Axum 0.8 + sqlx 0.9 +
socketioxide 0.18, mevcut şema (50 migration) değişmeden.

**Sözleşme testi: 72/72.** NestJS (4110) ve Rust (4120) aynı veritabanı ve JWT anahtarlarıyla
yan yana; aynı istek dizisi ikisine gönderilip cevaplar (değişken alanlar maskelenerek)
karşılaştırıldı:
- 48 HTTP adımı: durum kodları (POST → 201 dahil), hata gövdeleri (`code`, `message`,
  `field`, `context.safe`), doğrulama mesajı seçimi, 404 biçimi — aynı.
- Çapraz: Rust'ın argon2 özetleri NestJS'te, NestJS'inkiler Rust'ta doğrulanıyor; erişim ve
  yenileme jetonları iki yönde geçerli; 2FA bir sunucuda kurulup diğerinde kullanılıyor.
- socket.io (P2'nin kendi `socket.io-client` sürümüyle): onaylar, `presence:self/update`,
  bozuk jetonda `io server disconnect`, çıkışta OFFLINE — aynı.
- Kabul edilen tek sapma: bozuk JSON'da ayrıştırıcının ham hata metni (V8 ↔ serde);
  durum kodu ve gövde biçimi aynı.

Testin yakaladığı uyumsuzluklar (düzeltildi): validator.js `isEmail` sayı içeren TLD'yi
reddediyor; şemadaki `CURRENT_TIMESTAMP` varsayılanları saat dilimsiz sütunda yerel saati
yazıyor (Prisma ise UTC gönderiyor) — Rust zaman alanlarını her zaman açıkça UTC yazar.

**Yük testi** (kimlikli `GET /auth/me`, 64 eşzamanlı, 20.000 istek, yük üretici aynı makinede):

| | NestJS | Rust |
|---|---|---|
| İstek/sn | 945 | **4.349 (4,6×)** |
| p50 / p99 | 66 / 115 ms | **10 / 42 ms** |
| Bellek boşta → yük sonrası | 127 → 376 MB | **2 → 11 MB** |
| Sunucu CPU | 41,6 sn | **9,8 sn** |

Ek kazanç: sqlx 0.9 çalışma anında birleştirilen SQL metnini derleme hatası sayıyor
(SQL enjeksiyonuna karşı yapısal koruma).

### Spike 2 — Rust WebRTC katılımcısı (`rust/apps/p2-callbot`) ✅

Rust ile yazılmış bir arama katılımcısı, bugünkü P2 Electron istemcisiyle (0.9.9 kaynağı,
geliştirme modu) aynı birebir aramada. Sinyalleşme bugünkü NestJS sunucusu üzerinden;
protokol `CallProvider.tsx` ile aynı: `call:incoming/accept/state/signal/media`, kusursuz
müzakere (küçük kimlik "kibar"), `{description}`/`{candidate}`, Opus SDP ayarları.
Yığın: `libwebrtc 0.4` (LiveKit'in Google WebRTC sarmalayıcısı) + `rust_socketio`.

Test (`rust/tools/spike2/interop.mjs`, **7/7**): Electron ayrı profil + uzaktan hata
ayıklama portuyla açılır, giriş ve "Kabul et" sayfanın içinden yapılır (masaüstüne
sentetik giriş yok), Electron'da sahte mikrofon/kamera kullanılır.

| Yön | Sonuç (Electron `getStats` + bot ölçümü) |
|---|---|
| Bağlantı | `connected`, aynı makinede RTT ~0 ms |
| Rust → Electron ses | Opus, 535 paket / 12 sn, kayıp 0, jitter 1 ms |
| Rust → Electron görüntü (ekran paylaşımı olarak) | VP8 1280×720 @15, P2 arama sahnesinde "Rust Bot · ekran" kutusunda oynuyor |
| Electron → Rust ses | 100 kare/sn (10 ms'lik) |
| Electron → Rust görüntü (kamera) | 960×540 @~20 (Chromium uyarlıyor) |

Bulgular:
1. **WebRTC tek exe'de 25,5 MB** (bütün libwebrtc dahil). Kurulum hedefi (≤ 40 MB) için yer var.
2. **Windows'ta statik CRT zorunlu**: LiveKit'in hazır libwebrtc'si `/MT`. Workspace
   `+crt-static` ile derleniyor (`rust/.cargo/config.toml`); yan kazanç: kurulum Visual C++
   Redistributable istemiyor.
3. libwebrtc'de cihaz ses modülü (`create_device_audio_track`, gerçek mikrofon/hoparlör),
   ekran yakalama (`desktop_capturer`) ve ses işleme (`apm`: yankı/gürültü/kazanç) hazır —
   Faz 4'ün en riskli parçaları kütüphanede mevcut.
4. Rust tarafı kusursuz müzakerenin sade hali: kibar taraf hiç kendiliğinden teklif yapmaz,
   kaba taraf çakışan teklifi yok sayar. P2 istemcisinin tam uygulamasıyla uyumlu çalıştı;
   yeniden müzakere (ekran paylaşımı açıp kapama, ICE yeniden başlatma) Faz 4'te test edilecek.
5. Doğrulanmadı: TURN/röle üzerinden (farklı ağlar), 3+ kişilik mesh, ekran paylaşımı
   açma/kapama sırasında yeniden müzakere, gerçek mikrofonla yankı giderme.

### Spike 3 — Rust overlay host + bugünkü C++ DLL (`rust/apps/p2-overlay-host`) ✅

Bugünkü dağıtılan overlay DLL'i ve enjektörü (`native/overlay/dist`, değiştirilmeden) Electron
yerine Rust host'la çalıştı. Zincir: enjektör test oyununu (`apps/p2-d3dtest`, D3D11 / D3D12)
askıda başlatıp DLL'i yükler → Rust host paylaşımlı belleği kurar (`crates/p2-overlay-ipc`,
şema 4; C++ başlığıyla düzen MSVC `offsetof` çıktısıyla doğrulandı, derleme anı denetimli) →
overlay **ana uygulamayla aynı GPUI + p2-ui ile ekran dışında** çizilir (wgpu headless çizici)
→ önceden çarpılmış BGRA, kirli dikdörtgenle yazılır → DLL oyunun karesine bindirir → oyundaki
tıklama DLL → boru → host → GPUI düğmesine ulaşır.

| | D3D11 | D3D12 |
|---|---|---|
| Oyun FPS: overlay kapalı / açık / panel (girdi) | 143,9 / 143,8 / 144,7 | 143,9 / 142,2 / 144,1 |
| Overlay karesi: GPUI çizim + GPU→CPU okuma (ort / p95) | 8,5 / 17,2 ms | 8,7 / 17,0 ms |
| Küçültme + kirli dikdörtgen + yazma | 5,9 ms | 5,6 ms |
| Yazılan / DLL'in yüklediği kare | 338 / 338 | 340 / 340 |
| Kirli alan (ekranın %'si, ort) | 1,5 | 1,5 |
| Tıklama GPUI'ye ulaştı | evet | evet |

Görüntüler: `C:\p2-pg\spike3\` (`*-girdi.png`: oyun + P2 kartı + "Tıklandı ✓"; `*-dll-ekran-goruntusu.png`:
DLL'in overlay'siz karesi). Doğrulama yalnız test penceresini yakalar (Windows Graphics Capture),
oyuna giden fare mesajları yalnız o pencerenin kuyruğuna gönderilir (masaüstüne giriş yok).

Host maliyeti (30 kare/sn sürekli değişen kart): özel bellek 273 MB, CPU tek çekirdeğin ~%40'ı.
Yüksek; nedenleri ve Faz 7 çözümleri: (1) ekran dışı test penceresi sabit 2× ölçekli → 4 kat piksel
çizilip okunuyor (1× çizim için GPUI'de ölçek ayarı), (2) her karede CPU'da küçültme + tam
karşılaştırma (yalnız değişince çiz; GPUI hangi bölgenin değiştiğini zaten biliyor), (3) paylaşımlı
bellek 4K'ya göre ~127 MB ve her karede GPU→CPU→GPU kopyası (**7b: paylaşımlı GPU dokusu** bunları
kaldırır). Electron'un ekran dışı overlay'iyle karşılaştırmalı ölçüm henüz yapılmadı.

Bulgular:
1. **GPUI'nin Windows'ta ekran dışı çizicisi yok** (`current_headless_renderer` → `None`), ama
   wgpu tabanlı `WgpuHeadlessRenderer` Windows'ta derleniyor ve çalışıyor. Yalnız `test-support`
   özelliğinin arkasında ve deterministik test zamanlayıcısıyla geliyor; Faz 7'de bunu ürün kalitesine
   taşıyan ince bir katman (gerçek saat, 1× ölçek) gerekir.
2. **Yama: ekran dışı kare opak siyahla temizleniyordu** → kartın dışı oyunu örtüyordu. Saydam
   temizleme + önceden çarpılmış alfa yaması (`rust/third_party/gpui-pre-wgpu`), üst projeye önerilecek.
3. **Windows'un DirectWrite metin sistemi ekran dışı wgpu çizicisinde glif üretmedi**; cosmic-text
   (`CosmicTextSystem`, sistem fontları) sorunsuz — Türkçe karakterler dahil.
4. **PrintWindow flip-model DirectX pencerelerini yakalayamıyor** (bembeyaz döner); doğrulama ve
   ileride ekran görüntüsü yedeği için Windows Graphics Capture kullanılmalı.
5. Enjektörün çıktısı ilk satırdan okunmalı (pid); oyun süreci boru tutamacını devralabildiği için
   akış sonu gelmeyebilir (Electron tarafı da böyle yapıyor).
6. Paylaşımlı bellek oturumu kapanırken DLL'e bağlı boru okuması `CancelSynchronousIo` ile kesilmeli.
7. DLL testleri `%LocalAppData%\P2\overlay.log` dosyasına yazar (kurulu P2 ile aynı dosya); ayrı
   bir günlük yolu ortam değişkeni eklenmesi önerilir.
8. Doğrulanmadı: gerçek oyunlar ve hile önleyiciler, tam ekran (exclusive) mod, HDR/10-bit takas
   zincirleri, Vulkan/OpenGL, oyun içi klavye/IME girdisi.

---

## 9. Geçiş ilerlemesi

Yaklaşım: önce istemci, dikey dilimler halinde; her dilim bugünkü sunucuya bağlanan,
kullanılabilir bir Rust istemcisi bırakır. Sunucu (Faz 2) uyumluluğu Spike 4'te kanıtlandığı
için sonra taşınır. Bütün fazlar bitene kadar commit yok (`rust/faz-0` dalı, yerel).

| Dilim | Kapsam | Durum |
|---|---|---|
| 1 | Kabuk (çerçevesiz pencere, başlık), dil altyapısı (11 dil), API çekirdeği (401'de tek yenileme, hata biçimi, "ulaşılamıyor"), DPAPI şifreli oturum, kurulum türü, sunucuya bağlanamadık, ilk yönetici, giriş/kayıt/2FA/beni hatırla, kuruluş oluştur/katıl | ✅ gerçek sunucuya karşı otomatik pilotla doğrulandı (`rust/tools/client-smoke.ps1`): temiz profil → çalışma alanı 0,2–0,7 sn; kayıtlı oturumla doğrudan çalışma alanı; kapalı sunucuda "bağlanamadık". Bellek ~90 MB |
| 2 | Çalışma alanı: proje rayı, kanallar/kategoriler, sohbet (okuma/gönderme/düzenleme/silme/tepki), gerçek zamanlı bağlantı, kişiler ve çevrimiçi durum | ✅ `rust/tools/slice2-test.mjs` 8/8 (art arda 6 koşu): istemci yazar, karşı tarafın yaz/düzenle/tepki/sil olayları gerçek zamanlı gelir; ekran görüntüleri `p2-wincap` ile. Bellek ~96 MB özel (Electron ~390 MB), boşta ~%1 CPU |
| 2b | Dilim 2 kalanları: yazıyor göstergesi, yanıt/alıntı yazma, ileti, iş parçacıkları/forum, bahsetme ve emoji paneli, ekler, kanal tarayıcısı | ✅ `rust/tools/slice2b-test.mjs` 26/26: alıntılı yanıt, iş parçacığı paneli (gerçek zamanlı yanıt), dosya eki (yükleme, işaret biçimi, resim önizleme), @bahsetme önerisi (+ etkinlik), emoji paneli, kanala iletme, forum konusu + yanıtı, kanal tarayıcısı (ayrıl/katıl kalıcı). "Yazıyor" Dilim 3'te yapılmıştı |
| 3 | DM'ler, gruplar, profil, ayarlar | ✅ `rust/tools/slice3-test.mjs` 25/25 (Dilim 2 testi de 8/8): DM aç/yaz/düzenle/tepki/sil, arama kayıtları, "yazıyor", okunmamış DM ve grup rozetleri, grup kur/ayrıl/güncelle, profil paneli, kullanıcı menüsü (durum seçimi), ayarlar (profil adı/ünvan, durum, 2FA kur/kapat, açık/koyu tema kalıcı, dil), Ctrl+, ve Esc |
| 4 | Arama ve medya (Faz 4) | ✅ `rust/tools/slice4-test.mjs` 30/30 (Spike 2'nin Rust arama botuna karşı; gerçek mikrofon/hoparlör kullanılmadan): bireysel arama iki yönde (gelen/giden, zil, kabul/ret), sesli oda, iki yönde ses (Opus, ~-14 dBFS), sustur/sağırlaştır/bas-konuş, ekran paylaşımı iki yönde (1280×720 @15, yeniden pazarlıkla aç/kapa), ses ayarları (aygıtlar, mikrofon testi, işleme, seviyeler, kalite). Aramada 102 MB / ~%5 CPU |
| 4c | Arama kalanları | ✅ `rust/tools/slice4c-test.mjs` 14/14 (3 kişilik örgü: iki Rust istemcisi + arama botu): kamera, yayın kalitesi + izleyici başına kalite isteği, kişi başına ses, yönetici susturması, yalnız TURN röle. Bilgisayar sesi ve çizim kaplaması Dilim 12'de |
| 5 | Pano, belgeler, notlar, ekran görüntüleri, proje merkezi (Faz 5) | ✅ (pano, dokümanlar, etkinlik) `rust/tools/slice5-test.mjs` 23/23: görev panosu (pano sekmeleri, sprint/backlog kapsamı, kişi/metin süzgeci, sürükle-bırak kanban, liste, ayrıntı paneli + yorumlar, düzenleyici, sprint yönetimi, gerçek zamanlı), dokümanlar (klasörler, A4 sayfa okuma/düzenleme — Electron HTML biçimi kayıpsız, tuval: kaydır/yakınlaştır/sürükle/ekle/sil/metin/bağlantılar, paylaşım), etkinlik paneli. Notlar, ekran görüntüleri ve proje merkezi kütüphane sayfasının içinde olduğundan Dilim 6'da |
| 6 | Kütüphane / patcher (Faz 6) | ✅ `rust/tools/slice6-test.mjs` 20/20: `crates/p2-patcher` (CDC parçalama — electron/cdc.cjs ile bayt bayt aynı, çapraz testli; manifest ve kurulum planı; indir/birleştir/doğrula; duraklat/sürdür/iptal; hız sınırı; doğrula-onar; kaldır; başlat/durdur + oynama süresi; sürüm yükleme ve kaldığı yerden devam), kütüphane sayfası (Yükle/Güncelle/Oyna/Durdur, ilerleme, sürümler ve kategoriler, belirli sürümü kur, yayınla/canlıya al, yeni sürüm yükle, Aktivite/Notlar/Ekran görüntüleri). Güncellemede 9 MB'lık dosyada yalnız 2,6 MB indi. `library.json` Electron biçiminde |
| 7 | Oyun içi overlay (Faz 7) | ✅ `rust/tools/slice7-test.mjs` 23/23: kütüphaneden enjeksiyonla başlatma (alt süreç izleme, anti-cheat atlama, düz başlatmaya geri düşüş), `apps/p2-overlay` host (ekran dışı GPUI, kısmi kare), panel (görev ekle/durum değiştir, not ekle, performans + HUD, arama denetimi), oyunda bildirimler (hazır, DM, gelen arama), ekran görüntüsü (DLL arka tamponu), oyun kapanınca host durur ve oynama süresi yazılır. Gerçek D3D11 test oyununda, girdi yalnız oyun penceresine |
| 8 | Yönetim: yönetim paneli, roller ve yetkiler, erişim, kanal izinleri, proje/kanal/kategori yönetimi | ✅ `rust/tools/slice8-test.mjs` 21/21 (rol oluştur → yetki → kişiye ver → etkin yetki sunucuda; davet; kütüphane bayrağı; kuruluş adı; geri dönüşüm) + `slice8b-test.mjs` 27/27 (kategori/kanal oluştur-sil, varsayılan kanal, kanal izni → karşı taraf 403, erişim kapat/aç, proje oluştur/kütüphane/sil) |
| 9 | Uygulama kabuğu: tek örnek, p2://play, tepsi, kapat davranışı, masaüstü bildirimi | ✅ `rust/tools/slice9-test.mjs` 18/18: ikinci açılış argüman iletip çıkar, p2://play, kapat → sor → tepsiye (hatırlanır, pencere gizlenir), ikinci açılış pencereyi getirir, arka planda DM bildirimi, "çık" seçiliyken süreç kapanır |
| 10 | Kişisel ve sohbet kalanları: profil fotoğrafı + avatar görselleri, yıldızlılar, kanal bildirim tercihi, kanal içi arama, düzenleme geçmişi, resim görüntüleyici, grup üyeleri | ✅ `rust/tools/slice10-test.mjs` 17/17 |
| 11 | Ayarlar: Kütüphane (konum, hız, overlay kısayolları ve görünümü) ve Uygunluk (çalışma saatleri, otomatik durum) | ✅ `rust/tools/slice11-test.mjs` 11/11 (gerçek tuş yakalama dahil; saat dışı → Meşgul ekipte görünür) |
| 12 | Arama kalanları II: bilgisayar sesi paylaşımı, ekran üstü çizim (izleyici kalemi + yayıncının masaüstü kaplaması), arama balonu | ✅ `rust/tools/slice12-test.mjs` 40/40 (ses aygıtı AÇMADAN; `P2_NO_AUDIO_DEVICE`) + `cargo test` p2-media 9/9, p2-client 13/13: kalem — kendi pencereye PostMessage ile gerçek fare sürüklemesi → `call:signal {draw}` + `{cursor}` bota ulaşır; yayıncı — saydam, tıklama geçiren (WindowFromPoint alttaki pencereyi döndürür), en üstte kaplama penceresi botun çizgilerini ve adlı imlecini çizer (ekran görüntüsünde yeşil çizgi), yayın bitince kapanır; bilgisayar sesi — sentetik halka mikrofon kapısından sonra karışır (mikrofon kapalıyken bot yayın boyunca ~−20 dBFS alır, yayın bitince −120); balon — küçültünce belirir, 404×126, mikrofon/kulaklık/aç/ayrıl, × → Vazgeç / Bir daha gösterme (`call-widget.json`), Ayarlar > Uygulama anahtarı. **Gerçek WASAPI loopback yalnız birim testli** (bkz. notlar) |
| 13 | Açılış penceresi + istemci güncelleyicisi (`p2-updater::ClientUpdater`), oyun başlatma kartı, overlay görünümü host'ta (ölçek, saydamlık, gösterge yeri/ayrıntısı, hareketi azalt, bildirim türleri), kütüphane otomatik güncellemesi | ✅ `rust/tools/slice13-test.mjs` 48/48: sahte yayınla (Rust sunucusu :4151) `update:published` → indir → hazır → 20 sn sonra kendiliğinden kurulum (sahte başlatıcı, gerçek kurucu asla çalışmaz); açılış turu denetler/indirir/kurar ve ana pencereyi açmaz; güncelleme yoksa açılış penceresi kapanıp ana pencere açılır; geliştirme derlemesinde kapalı. D3D11 test oyununda ölçek 1.25 (ölçekli girdi doğru düğmeye gider), canlı görünüm değişikliği, `release:live` → oyun açıkken indirme + montaj oyun kapanınca, açılış denetimi, ayar kapalıyken indirmeme, 5 MB/s sınır; başlatma kartı overlay "hazır"da / düz başlatmada 4 sn sonra kapanır |
| S | **Faz 2 — Rust sunucusu (API + gerçek zamanlı)**: bütün NestJS modülleri (`apps/p2-server/src`: auth, organizations, preferences, roles, user_groups, channel_permissions, admin, projects, project_hub, notes, screenshots, direct, groups, uploads, documents, tasks, releases + release_store + release_manifest, updates, activity) ve `realtime.rs` + `calls.rs` (varlık, yazıyor, etkinlik, kütüphane duyurusu, belge imleci, bütün arama sinyalleşmesi) | ✅ Sözleşme testleri (`rust/tools/server/*-test.mjs`, aynı senaryo NestJS ve Rust'ta, maskeli yanıt + anahtar sırası + gerçek zamanlı olaylar + çapraz okuma): 14 paket, **1253/1253**. `cargo test` 23/23 (spec.ts karşılıkları). Rust istemcisinin bütün uçtan uca testleri Rust sunucusuna karşı (`rust/tools/server/e2e-all.mjs`): 13 paketin hepsi geçti (slice3 NestJS'e karşı da aralıklı zamanlama kaçırması yapıyor) |
| 8 | **Faz 8 — Dağıtım, güncelleme ve taşıma**: Windows kurulumları (`rust/packaging`: NSIS, "P2 Client" + "P2 Server", electron-builder ile aynı kimlik/klasör/kayıt/kısayol/argümanlar), `latest.yml` + yayın (`publish.mjs`), Linux sunucu paketi betiği, Electron 0.10.x köprüsü (`electron/migrate-bridge.cjs` → `migrate.json`) ve Rust ilk açılış içe alımı (`p2-client-core::migrate`: ayarlar, DPAPI oturum, kütüphane), exe simge + sürüm kaynakları, AppUserModelID | ✅ `rust/tools/faz8-test.mjs` 31/31 (hiçbir kurulum çalıştırılmadan): yayın → Rust sunucusu `/updates/{client,server}/latest.yml` + exe (sha512/boy, electron-updater ayrıştırıyor); **gerçek Electron** geçici profilde safeStorage oturumu + köprü `migrate.json` yazar → `p2-client --p2-import-electron` ayarları/oturumu/kütüphaneyi alır → istemci giriş ekranı göstermeden çalışma alanına açılır. `rust/packaging/verify-installer.mjs` 96/96 (paket açılıp her dosya bayt bayt, kayıt anahtarları/GUID, AUMID, kaldırıcı, `--updated`/`--force-run`/`/S`/`/D=`). `cargo test` p2-client-core migrate 5/5. Linux: çapraz araç yok → betik + belge (sahte ELF ile paketleme sınandı) |

Dilim 1 notları: otomatik pilot yalnız `P2_AUTOPILOT_*` ortam değişkenleriyle çalışır (masaüstüne
giriş göndermeden uçtan uca test); ayar dosyası UTF-8 BOM'a dayanıklı (duman testi yakaladı);
görsel denetim henüz elle yapılmadı.

Dilim 2 notları:
- **Abonelik yarışı.** Sunucu jetonu `handleConnection`'da eşzamansız doğrular; `subscribe` o
  bitmeden gelirse `{ok:false}` döner ve soket kuruluş odasına girmez (mesaj olayları gelmez,
  durum olayları gelir). İstemci `subscribe`'ı onaylı (ack) gönderir, `ok:true` gelene dek dener.
- **rust_socketio 0.6.** Varsayılan aktarım (polling + yükseltme) GUI sürecinde olayları ~25 sn
  (ping aralığı) geciktirdi; yalnız WebSocket kullanılıyor. Ayrıca ilk bağlantı ara sıra hiç paket
  almıyor; onay 3×800 ms içinde gelmezse soket bırakılıp hemen yenisi açılıyor (en kötü ~2 sn).
  Kök neden kütüphanede; Faz 2'de sunucu da Rust olunca istemci socketioxide uyumlu kendi ince
  istemcisine geçebilir.
- **Boşta yeniden çizim.** Yazma kutusu imleci her yanıp söndüğünde atası olan sohbet görünümü
  de kirleniyor, görünür bütün mesaj satırları yeniden çiziliyordu. Yazma kutusu önbellekli sohbet
  görünümünün kardeşi olarak çizilir (`ChatPane::composer_bar`).
- **Tanılama.** `P2_TRACE_SOCKET=1` (soket aşamaları ve ham olaylar, stderr),
  `P2_TRACE_RENDER=1` (saniyede görünüm çizim sayısı), arayüzsüz soket denemesi:
  `cargo run -p p2-client-core --example rt_probe -- <api> <kullanıcı> <parola>`.
- Kullanıcı menüsü (raydaki avatar): kim olarak girildiği + "Çıkış yap"; tek tıkla çıkış yok.

Dilim 2b notları (`screens/chat.rs` + `chat_extras.rs`, `screens/thread.rs`, `workspace/chat_tools.rs`,
`p2-client-core::chat_extra`):
- **Yanıt / alıntı.** Satır araç çubuğunda "Yanıtla": yazma kutusunun üstünde alıntı bandı (Esc
  ile vazgeç), gönderimde `quotedMessageId`. Kanal, DM ve grupta aynı.
- **İş parçacığı ve forum.** "Konu" (kanal/grup) ya da "N yanıt" bağlantısı sohbetin sağında
  ayrı bir panel açar (kendi yazma kutusu; önbellekli sohbet bölmesini kirletmez). Forum
  kanallarında üst düzey mesajlar konudur ("Konuyu aç"), yanıtlar `topics/:id/replies` ucuna
  gider. Yanıt olayları (`parentId` dolu) açık panele eklenir, değilse satırdaki sayaç artar.
- **Ekler.** 📎 ile çoklu dosya seçimi, arka planda yükleme (`/uploads`, data URL — Electron ile
  aynı), çipler, gönderimde `[[p2-file|…]]` işaretleri (attachments.tsx ile bayt bayt aynı biçim,
  birim testli). Resimler kimlikli indirilip çözülerek satır içinde önizlenir (en çok 360×240);
  diğer dosyalar "İndir" ile İndirilenler klasörüne kaydedilir.
- **@bahsetme ve emoji.** `@` yazınca proje üyelerinden öneri listesi (Enter ilki seçer, tıkla);
  😊 paneli imlece emoji ekler. Sunucu bahsetmeyi etkinlik olarak kaydeder (testte doğrulandı).
- **İlet.** Kanal mesajı aynı projedeki başka bir metin/duyuru kanalına (`/forward`).
- **Kanal tarayıcısı.** Kenar çubuğunda "Kanallara göz at": arama, Tümü/Katıldıkların/
  Ayrıldıkların; Ayrıl/Katıl `preferences` (`hidden`) ile kalıcı, ayrılınan kanallar kenar
  çubuğunda görünmez.
- **Henüz yok:** ileti geçmişi görünümü, kanal içi arama arayüzü (uç hazır: `search_channel`),
  yıldız/bildirim tercihleri arayüzü, sürükle-bırak ile ek, panodan resim yapıştırma, resim
  görüntüleyici (büyütme), emoji kısa kodları (`:smile:`), DM'de ekler için dosya önizleme.

Dilim 3 notları:
- **Tek sohbet bileşeni.** Kanal, DM ve grup uçları aynı ekleri kullandığı için (`/messages`,
  `/read`, `/messages/:id/reactions`) `ChatTarget` ile tek `ChatPane` üçünü de çizer; farklar
  başlıkta (kanal adı / kişi ve durumu / grup üyeleri ve "Gruptan ayrıl"), karşılamada ve yer
  tutucudadır. Gerçek zamanlı olaylar da `Scope` (kanal/DM/grup kimliği) ile tek yoldan akar.
- **Tepki biçimi.** Kanal uçları tepkileri özetlenmiş, DM/grup uçları ham (`{emoji,userId}`)
  döndürür; `Message::normalize` ikisini aynı biçime getirir.
- **Ayarlar.** Hesap ve Güvenlik, Görünüm, Dil ve Bölge, Kısayollar çalışıyor. Ses ve Görüntü
  (Dilim 4), Kütüphane (Dilim 6), Bildirimler, Uygulama (tepsi/kapatma — kabuk işiyle),
  Uygunluk (çalışma saatleri) listede "yakında" olarak duruyor. Dil seçimi kaydedilir; ekran
  metinlerinin çoğu henüz `p2-i18n` anahtarlarına bağlı değil (Türkçe sabit) — çeviri bağlama
  işi Faz 8 öncesi toplu yapılacak.
- **Kalanlar (2b ile birlikte):** profil fotoğrafı yükleme/kırpma, DM'de yıldız/bölünmüş
  görünüm/yeni pencere, gruba kişi ekleme/çıkarma arayüzü (API hazır), grup iş parçacıkları.
- Bilinen: soket kütüphanesinin "sağır ilk bağlantı" sorunu ara sıra bağlanmayı ~5 sn'ye
  uzatıyor (Dilim 2 notu); işlev etkilenmiyor.

Dilim 4 notları (`crates/p2-media`, `apps/p2-client/src/call.rs`):
- **Ses yolu.** libwebrtc'nin aygıt katmanı kullanılmıyor (LiveKit sürümü "Synthetic ADM"): mikrofon
  ve hoparlör `cpal` (WASAPI) ile, yankı/gürültü/kazanç libwebrtc APM ile, 10 ms'lik saatle.
  Uzak sesler karıştırılır ve APM'ye ters akış (yankı referansı) olarak verilir; zil ve işaret
  sesleri (notifications.ts notaları) aynı karışımda üretilir, dosya yok. Motor yalnız arama,
  zil ya da mikrofon testi varken açık; boşta WebRTC ve aygıtlar kapalı (boşta bellek değişmedi).
- **Bulunan iki libwebrtc tuzağı:** (1) `OfferOptions::default()` `offer_to_receive_*=Some(false)`
  — Unified Plan'da ses "sendonly" olur, karşı taraf ses göndermez (Spike 2 botunda da vardı;
  düzeltildi). (2) `NativeVideoSource` ilk gerçek kareye kadar 10 fps siyah kare basar; görüntü
  izi baştan eklenirse karşı tarafta siyah kutu çıkar. Çözüm: iz yalnız yayın açılınca eklenir,
  kapanınca sökülür (yeniden pazarlık; iki taraf da teklif verebilir, çakışmada kibar taraf geri
  alır — CallProvider.tsx ile aynı). `RtpSender::set_track(None)` imza iş parçacığını kilitliyor;
  kullanılmıyor.
- **Görüntü.** Gelen kareler I420→BGRA (libyuv) çevrilip GPUI `RenderImage` olarak sahnede
  oynuyor; eski kare dokusu her yenide bırakılıyor. Tıklanan yayın büyür.
- **Testler gerçek aygıt kullanmaz:** `P2_TEST_TONE` (sentetik 523 Hz mikrofon), `P2_SILENT_AUDIO`
  (hoparlör açılmaz), `P2_TEST_PATTERN` (ekran yerine desen; seçicide görünür).
- **Kalanlar (4c):** (çoğu yapıldı, bkz. Dilim 4c notları) kamera, bilgisayar sesi paylaşımı (loopback), yayın kalite menüsü
  (izleyici başına), ekran üstü çizim kaplaması, moderasyon menüsü, kişi başına ses seviyesi
  arayüzü, TURN zorlama ve 3+ kişilik örgü testi, Electron istemcisiyle canlı birlikte çalışma testi
  (protokol aynı; Spike 2 düzeneği yeniden kullanılacak).
- Exe 50 MB (GPUI + bütün libwebrtc); kurulum paketi sıkıştırılmış olarak hedefin altında kalmalı (Faz 8).

Dilim 4c notları (`p2-media::camera`, `engine.rs` kodlayıcı sınırları, `call.rs`, `call_ui.rs`):
- **Kamera.** Media Foundation kaynak okuyucusu (video işlemcisiyle RGB32 → I420); ayarlarda
  kamera seçimi. Electron gibi tek görüntü göndericisi: yayın açıksa yayın, değilse kamera gider;
  geçişte iz sökülüp eklenir (yeniden pazarlık). Kamera kaynağı "ekran yayını" kipinde değil:
  bant genişliği düşükken çözünürlük düşer, kare hızı korunur (testte 320×240@30).
- **Kalite.** Yayın seçicide 1440p/1080p/720p/480p; screenShare.ts ön ayarları ve `bitrateFor`
  birebir (birim testli). İzleyici kutucuktaki ⚙ ile Otomatik/1080p/720p/480p/360p ister
  (`call:signal {quality}`); gönderen o eşe giden kodlayıcıyı sınırlar (bit hızı, kare hızı,
  ölçek). Testte 1280×720 → 853×480. Sınırlar yeniden pazarlık bitince uygulanır: iz eklendikten
  hemen sonra `set_parameters` medya iş parçacığını ~10 sn bekletiyordu.
- **Kişi başına ses ve moderasyon.** Kutucukta −/+ (%0–200); sahip/yönetici için "Sustur"
  (`call:moderate` → karşı taraf `call:force-mute` ile susar).
- **Ağ.** "Yalnız TURN röle" (`forceRelay`): TURN tanımlıysa ICE yalnız röle adaylarıyla.
- **3 kişilik örgü** testte doğrulandı (iki Rust istemcisi + arama botu).
- **Henüz yok:** (bilgisayar sesi ve çizim kaplaması Dilim 12'de)
  kendi kamera önizlemen, Electron istemcisiyle canlı karışık arama testi (Faz 9).

Dilim 12 notları (`p2-media::loopback`, `engine.rs` `SystemAudio`, `apps/p2-client/src/{annotate,overlay_draw,call_widget}.rs`, `call.rs`, `call_ui.rs`, `screen_picker.rs`):
- **Bilgisayar sesi.** `native/loopback/src/loopback.cc` birebir: önce kendi süreç ağacımızı
  HARİÇ tutan WASAPI işlem loopback'i (Windows 10 20348+ / 11; karşı tarafın sesi yayına geri
  karışmaz), açılmazsa uç nokta loopback'i (cpal, varsayılan çıkış). 5.1/7.1 stereoya indirgenir
  (merkez/arkalar −3 dB), 48 kHz mono halkaya iner, halka 400 ms'yi aşarsa en eskisi atılır
  (`systemAudio.ts`). Karışım CallProvider.tsx `applyShareAudio` gibi tek ses izinde: mikrofon
  karesine **sustur/bas-konuş kapısından sonra** eklenir — mikrofon kapalıyken de yayının sesi
  gider. Açılamazsa "… Yayın sessiz başladı." Seçicide "Bilgisayar sesini paylaş" (settings.ts
  `shareAudio`, varsayılan açık, hatırlanır) + ScreenShareModal ipucu metni.
- **Bilgisayar sesi YALNIZ birim testli.** Geliştirme sırasında bu makinede işlem loopback'i
  (DAHİL kipi) sessize alınmış bir test sürecine bağlıyken Windows Ses hizmeti askıda kaldı
  (WER AppHangXProcB1, `Audiosrv` ↔ `audiodg`) ve yeni çalma akışları 0x800706CC ile açılmaz
  oldu. Bu yüzden gerçek ses G/Ç'si içeren hiçbir test yok: `cargo test -p p2-media`
  (`loopback::tests` indirgeme + karıştırıcı: toplama, doyurma, kısmi kare, 400 ms sınırı;
  `engine::system_audio_tests` sentetik halka enjeksiyonu: kapalı mikrofon karesine −23 dBFS
  RMS). Uçtan uca testte istemci `P2_NO_AUDIO_DEVICE` ile hiçbir ses aygıtını açmaz (mikrofon,
  hoparlör, loopback, aygıt listeleme) ve bilgisayar sesi motorun sentetik halkasından gelir —
  karışım ve gönderim yolu gerçek, yakalama sentetik. Gerçek loopback'in elle denenmesi gerekir
  (ses hizmeti yeniden başlatıldıktan sonra, kendi kullanım ortamında).
- **Çizim.** drawBus.ts / StreamPlayer.tsx: `call:signal {draw:{target,id,color,width,points}}`
  ve `{cursor:{target,name,color,x,y,gone?}}`, çizgiler 2,2 sn tam görünür + 2,6 sn'de solar,
  imleç 2,6 sn (son 600 ms solar), kişiye özgü kalem rengi (`penColor` aynı karma, birim testli),
  palet + kalınlık (2–14) `penInk`/`penWidth` olarak hatırlanır. Noktalar 60 ms'de bir, imleç
  55 ms'de bir gider. Kalem düğmesi (✎ "Ekrana çiz"/"Kalemi bırak") canlı yayın kutucuğunda;
  `screen.annotate` yetkisine bağlı (Electron istemcisi bu yetkiyi denetlemiyordu, sunucu
  kataloğunda var). Çizim sinyalleri medya motoruna gitmez, arayüzde kalır.
- **Masaüstü kaplaması.** main.cjs `showOverlay`: yalnız ekran paylaşımında (pencere
  paylaşımında hizalanamaz), libwebrtc ekran kimliği → `\.\DISPLAYn` → monitör dikdörtgeni.
  GPUI PopUp pencere + `WS_EX_LAYERED|TRANSPARENT|NOACTIVATE|TOOLWINDOW|TOPMOST` (tıklama
  geçirir, odak almaz, görev çubuğunda yok); ekran yakalandığı için izleyenler de çizimi görür.
  Testte (`P2_TEST_OVERLAY`) kaplama istemcinin kendi penceresinin üstüne konur.
- **Arama balonu.** main.cjs `syncCallWidget`/`WIDGET_HTML`: aramadayken ana pencere simge
  durumunda ya da tepside ise sağ altta 404×126 (P2 Mavi sabit renkleri, SVG simgeler aynı
  yollarla çizilir), "Sesli oda"/"Sesli arama", süre · kişi sayısı / "Çalıyor…", mikrofon ve
  kulaklık (Aç/Kapat), "P2'yi aç", ayrıl; balondan sürüklenir, konum `call-widget.json`
  (`{x,y,hidden}`, mantıksal px). × → Electron'daki yerel ileti kutusu yerine balonun içinde
  aynı üç seçenek ("Bu seferlik gizle" / "Bir daha gösterme" / "Vazgeç"). Ayarlar > Uygulama'da
  "Pencere küçültülünce arama balonunu göster".
- **Sapmalar / kalanlar:** kalem araç çubuğu Electron'daki gibi yalnız alt kenara gelince
  belirmiyor (hep görünür); kendi yayınının önizlemesi olmadığı için yayıncı kendi yayınına
  kutucuktan çizemez (kaplamada başkalarının çizimini görür); çizgilerin gölgesi bulanık değil
  (koyu, biraz kalın alt çizgi); balon "bu seferlik gizle" ve sürükleyerek taşıma uçtan uca
  test edilmedi (birim: durum dosyası biçimi); gerçek WASAPI loopback yukarıdaki nedenle
  yalnız birim testli.
- **Test notu.** Arama botu (kibar taraf olduğunda) ilk pazarlıkta görüntü teklif etmiyor; botun
  yayını Rust istemcisine ancak istemcinin ilk yayınıyla açılan görüntü hattından gelir. Bu
  yüzden testte önce yayıncı/kaplama, sonra izleyici kalemi denenir. (Rust istemcisi de teklifte
  boş görüntü alıcısı açmıyor — Electron'la karışık aramada Faz 9'da bakılacak.)

Dilim 8 notları (`screens/admin.rs`, `channel_perms.rs`, `workspace/manage_ui.rs`,
`p2-client-core::{admin, manage}`):
- **Yönetim paneli** (kullanıcı menüsü ve proje menüsünden): Genel Bakış, Ekip ve Yetkiler
  (Üyeler / Roller: rol oluştur, renk, yetki aç/kapat, kişiye ver/geri al, rolü sil; üyede
  oturumları kapat, sahipliği devret, çıkar, eski özel ayarları temizle), Erişim (kişi başına
  proje ve kanal görünürlüğü), Davetler, Kütüphane, Kanallar, Geri Dönüşüm, Kuruluş. Her bölüm
  `permissions/me` ile yetkiye bağlı (AdminPanel.tsx ile aynı kurallar). Onay pencereleri var.
- **Proje menüsü** (kenar çubuğu başlığındaki •••): kategori/kanal oluştur (dört tür), kanallara
  göz at, davet kodunu kopyala, proje ayarları (ikon, kapak, kütüphane), projeyi sil, yönetim
  paneli. Raydaki "+" proje oluşturur (kod Electron'la aynı kuralla üretilir).
- **Kanal sağ tık menüsü:** varsayılan kanal yap, kanal izinleri, sil.
- **Kanal izinleri:** Roller/Gruplar/Kişiler; her yetkide ✕ / ／ / ✓; kuralı sıfırla. Testte
  "Herkes" için mesaj engellenince rustbot gerçekten 403 aldı.
- **Henüz yok:** kullanıcı grupları (eski sistem; Electron'da da sekme gizli), kanal ve kategori
  yeniden adlandırma/sıralama (sürükle), görsellerin önizlemesi (ikon/kapak yalnız "seçildi"
  olarak görünür), proje adını değiştirme.

Dilim 9 notları (`apps/p2-client/src/shell.rs`, `app.rs`, `screens/settings_app.rs`):
- **Tek örnek.** Profil başına adlandırılmış boru: aynı profille ikinci açılış argümanlarını
  ilk örneğe yazıp çıkar; ilki pencereyi öne getirir. `p2://play/<proje>` ve `--p2-play=`:
  kuruluysa oyun başlar, değilse kütüphanede o proje açılır (main.cjs `playFromArgs`).
- **p2:// kaydı** (HKCU) yalnız kurulu sürümde yapılır; geliştirme derlemesi kurulu Electron
  istemcisinin kaydını ezmez (`P2_REGISTER_PROTOCOL` ile zorlanır). Asıl kayıt Faz 8 kurucusunda.
- **Kapat düğmesi:** Ayarlar > Uygulama: her seferinde sor / sistem tepsisine / simge durumuna /
  çık. "Sor" uygulama içi pencere açar (Electron'daki sistem iletişim kutusunun metinleri,
  "Bu seçimi hatırla"). Tepsi simgesi: tıkla → aç; sağ tık → "P2'yi aç" / "Uygulamadan çık".
- **Masaüstü bildirimi:** pencere öndeyken gösterilmez; DM, grup, bahsetme (`@kullanıcı`,
  `@everyone`) ve gelen arama. Tepsi balonuyla gösterilir (Windows 10/11 bildirim merkezine düşer;
  kayıtlı AppUserModelID gerekmez), görev çubuğu yanıp söner; tıklayınca pencere öne gelir.
  Ayarlar > Bildirimler: aç/kapat, ses. Otomatik pilotta (testler) masaüstüne balon, tepsi
  simgesi ve yanıp sönme çıkmaz; yalnız iz düşer.
- **Henüz yok:** arama balonu (pencere gizliyken küçük arama penceresi), açılış (splash)
  penceresi, Windows ile başlat, bildirime göre doğru sohbeti açma, bildirim sesinin kendisi.

Dilim 10 notları (`avatars.rs`, `settings.rs` profil fotoğrafı, `chat.rs` arama/geçmiş/görüntüleyici,
`workspace/group_members.rs`, `chat_tools.rs` tercihler):
- **Profil fotoğrafı:** Ayarlar > Hesap'tan yükle/kaldır (en çok 8 MB). Uygulama genelinde avatar
  önbelleği: kullanıcı kimliği → `/uploads/...` (üye, sohbet ve oturum listelerinden öğrenilir),
  kimlikli indirilip 128 px kare kırpılır; `p2_ui::avatar` bu önbelleği kaynak olarak kullandığı
  için bütün avatarlar kendiliğinden fotoğrafa döner, inince pencereler yeniden çizilir.
- **Yıldızlılar:** kanal (sağ tık menüsü) ve kişi (sağ tık) yıldızlanır; kenar çubuğunun başında
  "★ YILDIZLI" bölümü. **Kanal bildirimi:** Tüm mesajlar / Yalnız bahsetmeler (varsayılan) /
  Sessiz — masaüstü bildirimi buna uyar.
- **Kanal içi arama** (başlıkta 🔍), **düzenleme geçmişi** (düzenlenmiş kanal mesajında "Geçmiş"),
  **resim görüntüleyici** (görsele tıkla; Esc ya da tıkla kapanır).
- **Grup üyeleri:** grup başlığında "👥 Üyeler": proje üyelerinden ekle, çıkar.
- **Henüz yok:** fotoğraf kırpma arayüzü, görüntüleyicide yakınlaştırma/kaydırma ve PDF, arama
  sonucuna tıklayınca mesaja gitme, DM'de geçmiş (sunucuda yalnız kanal için var).

Dilim 11 notları (`screens/settings_library.rs`, `p2-overlay-client::hotkeys`, `overlay_ui.rs`):
- **Ayarlar > Kütüphane:** kurulum konumu, indirme hız sınırı (patcher'a uygulanır), otomatik
  indir, oyun içi overlay aç/kapat (kapalıyken düz başlatılır), oyun açılınca küçült, etkinlik
  paylaşımı/duyuru/bildirim anahtarları; overlay kısayolları (yakalama: "Değiştir" → tuşa bas,
  Esc vazgeç, boş bırakılabilenlerde Backspace kapatır; çakışma uyarısı), bildirim köşesi, saydamlık,
  ölçek, animasyonları azalt, HUD (açık başlar) ve yeri, ayrıntılı HUD, oyun içi bildirim türleri,
  ekran görüntüsü klasörü. Kısayol biçimi ve VK tablosu src/hotkeys.ts ile birebir (birim testli);
  bas-konuş oyuna da iletilir (bit7), 4 = bas-konuş, 5 = sağırlaştır.
- **Ayarlar > Uygunluk:** çalışma günleri ve saatleri; saat dışında durumu değiştirme / Uzakta /
  Meşgul. Dakikada bir denetlenir; kendi koyduğu durumu saat içine dönünce Çevrimiçi'ne alır.
  Otomatik pilot testleri gerçek saate göre durum değiştirmez (`P2_FAKE_CLOCK` ile sınanır).
- **Henüz yok:** overlay'de saydamlık/ölçek/animasyon azaltma/HUD yeri ve bildirim türlerinin
  host tarafında uygulanması (ayar saklanıyor; host şimdilik HUD ve köşeyi uyguluyor), otomatik
  indirme akışının kendisi, "etkinliği paylaş/duyur" sunucu olayları, disk boş alanı göstergesi.
  (Görünüm/bildirim türleri ve otomatik indirme Dilim 13'te yapıldı.)

Dilim 13 notları (`apps/p2-client/src/{splash,updates}.rs`, `screens/{auto_update,launch_splash}.rs`,
`overlay_ui.rs`, `settings_app.rs`; `apps/p2-overlay` + `p2-overlay-ipc::proto::Look`):
- **İstemci güncelleyicisi** (`updates.rs`, `p2_updater::ClientUpdater`; updater.cjs + UpdateBanner/
  UpdateSetting karşılığı): veri klasörü profil (`update-source.json`, `.updaterId`), sürüm kök
  package.json'dan (0.9.9). Çalışma alanı kurulunca `configure(api)`, `update:published` → `check()`,
  aramadayken `set_busy`, overlay'li oyun açıkken boşta sayılmaz (`set_idle_check`), kapanışta
  `install_if_ready()` (`on_app_quit`). Ayarlar > Uygulama > "Sürüm ve güncelleme": sürüm rozeti, canlı
  durum, "Güncellemeleri denetle" (2,5 sn sonra not), hazırsa "Yeniden başlat ve kur" (arka planda).
  Geliştirme derlemesinde (`p2-rust-target`) `packaged=false`: hiçbir şey yapılmaz. `P2_UPDATER_TEST=1`
  testte açar; o kipte kurucu çalıştırılmaz (sahte başlatıcı iz + `update-install.txt`), önbellek
  profilde. TODO (Faz 8): eski birleşik kurulumun `before_install` adımları (schtasks "P2 Server",
  `--p2-server` süreçleri, HMAC'li `/updates/stop`).
- **Açılış penceresi** (main.cjs `showSplash`/`SPLASH_HTML`): 300×340, çerçevesiz, her zaman üstte,
  sürüklenebilir; ana pencereden önce açılır, `startup_check(saved_base())` sürerken `state.text`
  ("Güncellemeler denetleniyor…" → "Güncelleme indiriliyor · %N" + dolan çubuk → "Güncelleme
  kuruluyor, uygulama yeniden başlayacak…"); kurulum başlarsa uygulama açılmaz. Yalnız paketlenmiş
  sürümde ve kayıtlı kaynak varken. Logo `build/icon.png` (gömülü), `p2-logo-pop`, `p2-pop-elastic`
  metin, belirsiz `.p2-bar` (1,3 sn). Radyal zemin eş merkezli halkalarla yaklaşık; harf aralığı ince
  boşlukla. Ana pencere açıldıktan SONRA kapanır (GPUI son pencere kapanınca çıkıyor — ilk denemede
  uygulama kapandı).
- **Oyun başlatma kartı** (patcher.cjs `showLaunchSplash`): 440×150, saydam, odak almaz, her zaman üstte;
  baş harf ikonu, "P2 · Başlatılıyor", ad, belirsiz çubuk, parıltı. Overlay "hazır"da, overlay kare
  alamazsa (+"… çalışıyor" bildirimi), düz başlatmada 4 sn sonra (+"… başlatıldı" bildirimi), oyun
  kapanınca ya da en geç 20 sn'de kapanır. Proje ikonu görseli yok (baş harfler).
- **Overlay görünümü:** `ToHost::Look` (init'te `look`, değişince anında). Ölçek: GPUI penceresi
  `w/ölçek` mantıksal boyutta çizilir, kare alan ortalamasıyla `w×h`'ye örneklenir, fare koordinatı
  ölçeğe bölünür (test: 1.25'te 2. düğme doğru açıldı); değişince yüzey yeniden kurulur. HUD
  (`.ow-hud`): 28 px hap, sol üst/üst orta/sağ üst, FPS · ms (GET /health 5 sn, kayıp son 24 yoklama)
  · ayrıntılıda CPU/RAM/%kayıp, düşük değer uyarı rengi; aynı köşedeki bildirimler altına iner.
  Saydamlık panel kapalıyken sabit kalan HUD'a uygulanır (Electron'da sabitlenen araç pencerelerinin
  varsayılanı — Rust'ta sabitlenebilir pencere yok). Bildirim girişi `p2-slide-right-elastic` /
  `ow-slide-left-elastic`; "animasyonları azalt" açıkken anında. Bildirim türleri: mesaj, arama,
  sürüm (yayınlandı/canlı), indirme bitti istemcide süzülür.
- **Otomatik güncelleme** (library.ts `useLibraryAutoUpdate`): açılışta kurulu her projenin
  `releases/latest?channel=` denetimi, `release:live` → karar (sabitlenmemiş, aynı kategori, farklı
  sürüm, ayar açık), kuyruk tek tek; oyun açıkken indirme sürer, montaj kapanınca (`waiting-for-exit`).
  `release:published` masaüstü/oyun içi bildirim. Realtime olay listesine `release:*` ve
  `update:published` eklendi (önceden hiç dinlenmiyordu).
- **Düzeltme:** hız sınırı 8 kat düşük uygulanıyordu (MB/s yerine Mbit/s); patcher.cjs gibi
  `kbps = mbps × 1024`.
- **Henüz yok:** "Yeni sürüm hazır" penceresi ve indirme şeridi (UpdateBanner; otomatik kurulum ve
  ayarlardaki düğme çalışıyor), aramada WebRTC RTT'nin HUD'a gelmesi (şimdilik /health), GPU ölçümü,
  oyun içi bildirim kartlarında düğmeler, ekip "oyuna girdi/çıktı" ve görev atama oyun içi kartları,
  disk boş alanı göstergesi.

Dilim 5 notları (`apps/p2-client/src/screens/{board,board_editor,documents,activity_ui}.rs`,
`crates/p2-client-core/src/{board,documents,activity}.rs`):
- **Pano.** Sürükle-bırak GPUI'nin `on_drag`/`on_drop`'u ile: kartın üstüne bırakınca onun önüne,
  sütuna bırakınca sona; iyimser güncelleme + `PATCH …/move { status, order }`. Kapsam seçimi
  BoardPage.tsx ile aynı (aktif sprint, yoksa backlog; BACKLOG durumu "Yapılacak"ta görünür).
  Yetki: sprint açmak/başkasının görevini silmek "Panoyu yönet" ister (sunucu denetliyor;
  istemci düğmeleri kuruluş rolüne göre gösteriyor — ayrıntılı yetki kataloğu Faz 6 yönetim
  paneliyle).
- **Sayfa dokümanı.** Electron `contentEditable` HTML'inin kullanılan alt kümesi (p/h1–h3/
  blockquote, b/i/u, bağlantı, hizalama, `<br>`) blok yapısına çevrilir; düzenleme satır başına
  bir blok + sade biçimle (`# `, `**…**`, `[orta] `…). Gidiş-dönüş testle kayıpsız. Alt küme
  dışında biçim (görsel, renkli span, tablo) içeren sayfa Rust'ta salt okunur açılır — Electron
  biçimi asla bozulmaz. Okuma görünümü A4 kâğıt.
- **Tuval.** Düğümler ve bağlantıların bilinmeyen alanları (`emoji`, `fontFamily`, yön, ızgara…)
  `#[serde(flatten)]` ile korunur. Nokta ızgarası, üçgenler ve bezier bağlantılar tek `canvas()`
  katmanında çizilir. Henüz yok: bağlantı çizme/silme, düğüm boyutlandırma, görsel düğümlerin
  resmi (yalnız dosya adı), canlı imleçler, sayfa görsel ekleri, mention menüsü.
- **Etkinlik.** Raydaki zil + okunmamış rozeti; tıklayınca kanal/grup/pano/dokümanlara gider.

Dilim 6 notları (`crates/p2-patcher`, `apps/p2-client/src/screens/library.rs`):
- **Uyumluluk.** CDC (Gear tablosu, maskeler, MIN/AVG/MAX) electron/cdc.cjs'nin birebir
  karşılığı; test JS'ten alınan kesim uzunlukları ve özetlerle karşılaştırıyor, uçtan uca test de
  Rust'ın yüklediği manifesti cdc.cjs çıktısıyla denetliyor. Bu sayede Rust ile yüklenen sürüm
  Electron'da, Electron ile yüklenen Rust'ta kurulur; parçalar tekilleşir.
- **Disk düzeni Electron'la aynı:** `<kök>/<klasör>/.p2/manifest.json`, `.p2/chunks/` (indirme
  önbelleği), `library.json` (bilinmeyen alanlar korunur). Faz 8 taşıması kurulu oyunları
  yeniden indirmeden devralabilir. Fark: "kurulu sürüm" alanına manifestin biçim sürümü (`1`)
  yerine sürüm numarası yazılıyor.
- **Henüz yok:** başlatma "splash" penceresi,
  kısayol oluşturma, sürüm farkı (diff) görünümü, taslak düzenleme/silme, başarım/inceleme
  yazma, notlarda sabitleme/çakışma birleştirme, ekran görüntüsü yükleme (overlay'den gelir),
  devam eden yüklemeler listesi (durum dosyası yerine sunucunun "missing" listesiyle devam).
- Ekran görüntüsü küçük resimleri kimlikli indirilip çözülüyor (GPUI'de uzak resim yükleyici yok).

Dilim 7 notları (`apps/p2-overlay`, `crates/p2-overlay-client`, `screens/overlay_ui.rs`):
- **Mimari.** C++ DLL + enjektör aynen kullanılır (şema 4). Electron'daki ekran dışı
  BrowserWindow yerine her enjekte edilen oyun süreci için ayrı bir `p2-overlay` süreci: ekran dışı
  GPUI (wgpu headless + cosmic-text), 2× çizip küçültür, önceden çarpılmış BGRA ve kirli
  dikdörtgenle yazar. İstemciyle satır başına JSON (`p2-overlay-ipc::proto`: init/token/toast/
  dismiss/visible/call/hotkeys/shot/stop ↔ session/ready/visible/hotkey/action/shot/log).
  Ayrı süreç şart: bir süreçte tek GPUI uygulaması var ve host çökse bile istemci etkilenmez.
- **Başlatma.** Kütüphane `Patcher::set_launcher` kancasıyla overlay yöneticisine sorar; ikililer
  yoksa, anti-cheat izi bulunursa ya da enjektör başlatamazsa düz başlatılır. Host süreci oyundan
  ÖNCE başlatılıp bekletilir (soğuk açılış DLL'in 5 sn'lik bağlanma penceresini kaçırmasın — ilk
  denemede yaşandı). Alt süreçler saniyede bir taranır (Toolhelp), 64-bit ve yardımcı olmayanlara
  `--attach` ile enjekte edilir; oynama süresi tüm ağaç bitince yazılır.
- **Panel.** Araç çubuğu (Görevler, Notlar, Performans, Arama, Ekran görüntüsü, HUD) + araç kartı.
  Görevler: projenin ilk panosu, yazıp Enter ile ekle, durum çipine tıklayınca sonraki duruma geç.
  Notlar: liste + hızlı özel not. Performans: FPS (DLL Present sayacı), son 60 sn çubuk grafiği,
  ortalama/en düşük %1, oyun ve sistem CPU/RAM; köşeye sabitlenebilir HUD. Arama: katılımcılar,
  konuşan vurgusu, sustur/sağırlaştır/ayrıl (istemciye eylem olarak döner). Esc paneli kapatır.
- **Girdi.** DLL'in fare/tuş/karakter mesajları GPUI olaylarına çevrilir (çift tık sayımı,
  tekerlek, vekil çiftler, Ctrl kısayolları); metin alanları gpui-component `Input`.
- **Çizim maliyeti.** Panel kapalı ve bildirim/HUD yokken hiç çizilmez; bildirim/HUD yalnız
  değişince (HUD 1 Hz), panel açıkken etkileşimden sonra 30 fps, yoksa 2 Hz.
- **Bildirimler.** Oyun hazır (panel kısayol çipleriyle), DM/grup mesajı, gelen arama (yapışkan,
  arama başlayınca/bitince kalkar), ekran görüntüsü. Kısayollar Electron'la aynı: Shift+2 panel,
  F12 ekran görüntüsü (Resimler\P2\<proje>), Shift+M sustur.
- **Belirteç.** Host yalnız erişim belirteci taşır (`Api::set_access_only`, yenileme kapalı —
  yenileme belirtecini döndürüp istemcinin oturumunu düşürmesin); istemci 30 sn'de bir ve 401'de
  yenisini iletir.
- **Henüz yok:** sohbet/ekip/tarayıcı araçları, sabitlenebilir araç pencereleri, ekran
  görüntüsünü paylaş/göreve ekle düğmeleri, imleç şekli (yuva hazır: `Session::set_cursor`),
  ayarlardan kısayol değiştirme, başlatma "splash" penceresi, GPU ölçümü.

Faz 2 (Rust sunucusu) notları (`apps/p2-server`, testler `rust/tools/server/`):
- **Çekirdek.** `ctx.rs` (kimlik çıkarıcı `Me`, NestJS durum kodları, `iso!` = Prisma tarih biçimi,
  `page_args`), `perm.rs` (permissions.ts birebir), `activity.rs` (`record`), `realtime.rs` yayın
  yardımcıları (`emit_users/org/channel/project`, `broadcast`). Prisma çıktısı `json_build_object`
  ile (jsonb değil — anahtar sırası korunur). Zaman damgaları her zaman `now()` ile UTC yazılır.
- **Sözleşme testi kurgusu.** `lib.mjs` → `startRust(port)` exe'yi kopyalayıp repo kökünden çalıştırır;
  aynı senaryo iki sunucuda taze varlıklarla koşar, uuid/tarih/jeton maskelenir, anahtar sırası dahil
  karşılaştırılır; Rust'ın yazdığı NestJS ile okunur. Dosya sistemi kullananlar (sürümler,
  güncellemeler) geçici klasörlerle ayrı bir NestJS açar.
- **Bilinçli kopyalanan tuhaflıklar.** `null` alanlarda TS'in 500'e düşmesi, ILIKE'ta `_`/`%` joker
  davranışı, boş PATCH'te `updatedAt`'in değişmemesi, sırasız Prisma ilişkilerinde aynı SQL planı.
- **Sapmalar.** Bilinmeyen yöntemde 405 yerine NestJS'in "Cannot METHOD" 404'ü çoğu yerde var, statik
  yol + yanlış yöntemde bazı uçlar 405 döner; Türkçe sıralama (`localeCompare('tr')`) ICU'suz
  yaklaşık; bir istekteki gerçek zamanlı olaylar NestJS'teki gibi ateşle-unut değil, sırayla.
  Metin uzunluğu doğrulaması validator.js `isLength` (vekil çifti / varyasyon seçicisi) ile aynı.
- **main.rs.** 340 MB gövde sınırı (main.ts), açılışta `releases::start` + `updates::start`
  (onModuleInit), bağlantı adresi (`/updates/stop` yalnız yerelden).


Faz 8 notları (`rust/packaging/`, `electron/migrate-bridge.cjs`, `crates/p2-client-core/src/migrate.rs`):
- **Kurulum (NSIS, `rust/packaging/nsis/p2-installer.nsi`).** electron-builder 26.15.3 şablonlarının
  (assisted, perMachine:false) tek geçişli karşılığı; makensis 3.0.4.1 + eklentiler (StdUtils, UAC,
  WinShell) electron-builder önbelleğinden, StdUtils/UAC başlıkları `node_modules/app-builder-lib`'den.
  Kimlik birebir: appId `com.p2.desktop`/`com.p2.server`, GUID = UUID v5(appId, electron-builder ad
  alanı) → istemci `56aca962-14fc-517c-9ad3-985363b86560` (bu makinedeki kurulu Electron'un
  `HKLM\…\Uninstall` anahtarıyla aynı, salt okunarak doğrulandı), sunucu `332f3932-…`. Klasör
  `%LOCALAPPDATA%\Programs\P2 Client` (ya da var olan kurulumun yeri; makineye kurulumsa Program Files +
  UAC), exe adı `P2 Client.exe` / `P2 Server.exe`, kaldırıcı `Uninstall P2 Client.exe`, kayıt değerleri
  (InstallLocation, KeepShortcuts, ShortcutName, UninstallString `/currentuser|/allusers`, …), kısayollar
  "P2" / "P2 Server" + `WinShell::SetLnkAUMI`. Argümanlar: `--updated` (sayfalar atlanır, eski kaldırıcı
  `--updated` ile veri silmeden çağrılır), `/S`, `--force-run` (sessizken kurulumdan sonra kullanıcı
  yetkisiyle açar), `/D=` (boşluklu, sonda), `--keep-shortcuts`, `--delete-app-data`,
  `--no-desktop-shortcut`. Yükseltme Electron'un kendi kaldırıcısını `/S /KEEP_APP_DATA /<kip>
  [--keep-shortcuts] --updated _?=<klasör>` ile çağırır (electron-builder ile aynı); kısayollar ve görev
  çubuğu sabitlemesi korunur, `%APPDATA%\P2` silinmez.
- **Bilinçli farklar.** Kaldırıcı derlemede değil kurulumda `WriteUninstaller` ile yazılır
  (electron-builder ara bir exe çalıştırır; bu makinede hiçbir kurulum exe'si çalıştırılmadı); uygulama
  7z yerine katı LZMA ile gömülü (istemci 20,5 MB, Electron ~180 MB); "kimin için" sayfası yok (var olan
  kurulumun kipi izlenir, yoksa kullanıcıya; `/allusers` ile makineye); diller TR + EN (electron-builder
  26); `%LOCALAPPDATA%\p2-desktop-updater\installer.exe` kopyası (yalnız electron-updater'ın fark
  indirmesi için) yazılmaz; blockmap üretilmez (electron-updater tam indirmeye düşer; `publish.mjs` eski
  blockmap'i siler ki yanlış fark denenmesin). Kaldırmada p2:// kaydı bu kuruluma işaret ediyorsa silinir.
- **Sunucu paketi.** `P2 Server.exe` (konsol), `p2-server.exe`, WinSW `P2ServerService.exe` +
  `p2-server-install.bat`/`-remove.bat`/`prepare-service-config.ps1` (`rust/deploy/windows`). Kurulum
  `P2ServerService.xml`'i mutlak yollarla yazar (ps1 ile aynı içerik, UTF-16 BOM). ProgramData\P2'ye ve
  hizmet kaydına dokunulmaz. Electron döneminde kayıtlı hizmet `"P2 Server.exe" --p2-server`
  çalıştırıyordu: Rust konsolu bu argümanla açılırsa işi `p2-server.exe`'ye devreder (çıkış kodu
  iletilir) — hizmet yeniden kaydedilmeden yeni sürüm çalışır.
- **Sürüm.** Exe'ler sürümü derlemede gömer (`package.json`, ya da `P2_PRODUCT_VERSION` —
  `build-installers.mjs --version`); paketleyici exe'nin VersionInfo'su kurulum sürümüyle aynı değilse
  durur (yoksa güncelleyici aynı sürümü sonsuza dek kurar). Exe kaynakları: istemci `build/icon.ico`,
  konsol `icon-server.png`'den ICO, üçünde de VERSIONINFO (CompanyName "P2 Team", ProductName,
  LegalCopyright — Electron exe'siyle aynı alanlar). İstemci sürüm derlemesi artık Windows alt
  sisteminde (konsol penceresi açılmaz; testler izleri yine borudan okur).
- **AppUserModelID.** İstemci `com.p2.desktop` (geliştirme derlemesi `.dev`), konsol `com.p2.server`
  (`SetCurrentProcessExplicitAppUserModelID`) — kısayollardaki kimlikle aynı, sabitlenmiş simge aynı
  düğmede açılır. p2:// kaydı uygulamanın kendisinde (HKCU, `"URL:p2"`, `"<exe>" "%1"` — Electron
  `setAsDefaultProtocolClient` ile aynı); exe yolu değişmediği için yükseltmede kesintisiz.
- **Yayın (`publish.mjs`).** `scripts/publish-update.mjs` ile aynı düzen: `client/` (+ kökte ayna),
  `server/` (+ exe kökte); önce exe, en son latest.yml. latest.yml electron-builder biçiminde birebir
  (`version`, `files[url, sha512 base64, size]`, `path`, `sha512`, `releaseDate` tek tırnak); hem
  p2-updater hem electron-updater (`parseUpdateInfo`/`resolveFiles`) kabul ediyor. **Geçiş sırası:**
  0.10.x Electron köprü sürümü yayınlanır (herkes bir kez açsın: `migrate.json` yazılır) → Rust sürümü
  daha büyük sürüm numarasıyla (`--version 0.11.0`) yayınlanır; Electron güncelleyicisi onu indirir ve
  `--updated /S --force-run` ile kurar (tek tık).
- **Taşıma köprüsü.** Electron 0.10.x: `migrate-bridge.cjs` ana pencerenin bütün localStorage'ını
  `%APPDATA%\P2\migrate.json`'a yazar (yüklemeden 3 sn sonra, dakikada bir değiştiyse, kurulumdan önce
  `updates.setBeforeInstall`). Jeton bu dosyada yok. Rust ilk açılışta (profil boşsa:
  settings.json/session.bin/`electron-import.json` yok) `p2:settings`/`p2:apiUrl`/`p2:mode`'u
  `Settings`'e çevirir (dil, kapatma, bildirim, ses — aygıt kimlikleri Chromium karması olduğu için
  varsayılana döner, `ptm` → açık mikrofon, `KeyB` → `b` —, kütüphane, overlay, uygunluk), temayı
  `appearance.json`'dan alır, ham localStorage'ı `electron-localstorage.json` olarak saklar (pano
  görünümü, proje sırası, overlay düzenleri — Rust karşılığı gelince okunacak). Oturum: Electron
  `safeStorage` = Chromium OSCrypt `v10` (AES-256-GCM, anahtar `Local State` → DPAPI); Rust anahtarı
  DPAPI ile, jetonu Windows CNG (BCrypt) ile çözer, kendi DPAPI biçiminde yazar — yeni bağımlılık yok,
  düz metin diske inmez. `library/library.json` (biçim aynı), `hash-cache.json`, `uploads/`,
  `call-widget.json` kopyalanır; oyun dosyaları kökte kaldığı için kurulu oyunlar olduğu gibi görünür.
  Electron profili değiştirilmez (geri dönülebilir). Köprü sürümü atlandıysa eski `legacy-prefs.json`
  denenir; oturum ve kütüphane yine de taşınır.
- **Karar: Rust profili `%APPDATA%\P2-Rust` kalır.** Electron profilinin (Chromium dosyaları, Local
  State) üzerine yazılmaz, sorun çıkarsa Electron'a dönmek veri kaybettirmez; Faz 9'da Electron
  emekliye ayrılınca `%APPDATA%\P2` temizlenebilir.
- **Güvenlik (testler).** `P2_PROFILE_DIR` verilmişse gerçek `%APPDATA%\P2` hiç okunmaz; kaynak yalnız
  `P2_ELECTRON_PROFILE_DIR` ile verilir. `P2_NO_ELECTRON_IMPORT=1` taşımayı kapatır.
- **Linux sunucu paketi.** `rust/packaging/linux/build-linux-server.mjs`: Linux'ta yerel derleme,
  Windows'ta `cargo zigbuild` ya da hedef + `P2_LINUX_LINKER` ile çapraz; hiçbiri yoksa yol gösterip
  çıkar. `--binary` ile hazır ikiliden `P2-Server-Ubuntu-24.04-x64-<v>.tar.gz` (+ .sha256) üretir;
  CRLF ile çıkış yapılmış betikleri LF'ye çevirir. Bu makinede Linux hedefi/bağlayıcı yok: paketleme sahte
  bir ELF ile sınandı, gerçek ikili Ubuntu/CI'da derlenmeli.
- **Kalanlar.** Kod imzalama yok (Electron'da da yoktu; SmartScreen uyarısı aynı). Kurulum paketleri bu
  makinede çalıştırılmadı (kural): gerçek yükseltme (Electron 0.9.9/0.10.x → Rust, makineye kurulum +
  UAC, hizmetin yeni XML ile yeniden başlaması) temiz bir test makinesinde/VM'de bir kez denenmeli.
  WinSW ve NSIS eklentileri hâlâ `node_modules`/electron-builder önbelleğinden alınıyor; Faz 9'da
  Electron kaldırılırken `rust/packaging/vendor`'a alınmalı. Eski birleşik kurulumun
  (`schtasks "P2 Server"`) güncelleme öncesi adımları (`updates.rs` TODO) yapılmadı.

Faz 9 — görsel parite temeli (2026-11-13):
- **Araç** `rust/tools/parity/` (README): Electron geliştirme modu yalıtılmış geçici profilde (`electron-main.cjs`: userData,
  p2://, kısayol, schtasks/sc/powershell, bildirim, güncelleyici etkisiz; CDP ile gezinme) + Rust istemcisi (`P2_PROFILE_DIR` +
  `P2_AUTOPILOT_*`, `p2-wincap --client` istemci alanı) → 1280×800 ölçek 1, fark %, HTML rapor ve geçmiş
  (`C:/p2-pg/parity`). 61 ekranlık katalog; `dom-styles.mjs` hesaplanmış stil dökümü; tohum kanalı `#parite`.
- **Temel**: `p2-ui` bütün `--p2-*` token'ları + `theme.ts paletteFor()` (19 tema; 722 değer Electron çıktısıyla testli),
  gömülü Inter ("Inter Variable Text"), yazı ölçeği 0,85 (`font_size::*`, `fs()`), `line-height: normal` 1,21, kaydırma çubuğu,
  gölgeler (içe dahil). Eşleme ve GPUI sınırları (harf aralığı, radyal gradyan, backdrop blur): `rust/tools/parity/TOKENS.md`.
- Yeni görünüm kancaları: `P2_AUTOPILOT_AUTH_VIEW`, `_PROJECT_MENU`, `_CHANNEL_MENU`, `_SCROLL`, `_DRAFT`, `_EMOJI_PANEL`, `_MENTION_POPUP`.
- **A (kabuk + giriş + tasarım sistemi)**: DirectWrite yaması (harf aralığı `P2ls`, değişken `wght`/`opsz` eksenleri, gri tonlama yumuşatma);
  `p2_ui::{controls, form, glyphs, text}` + `components::{logo_image, radial_background}` (TOKENS.md §5–6); özel başlık çubuğu
  (☰ uygulama menüsü, pencere düğmeleri), giriş/kayıt/2FA/kurulum türü/sunucu sorunu/ilk yönetici yeniden yazıldı, uygulama içi
  toast yığını (`toasts::push`) ve güncelleme şeridi/penceresi eklendi. Parite: giriş %1,1, kayıt %1,3, 2FA %1,7, kurulum türü %0,5,
  sunucu sorunu %0,7, toast %1,5 (koyu; açık tema benzer). Yeni kancalar: `P2_AUTOPILOT_APP_MENU`, `_TOAST`, `_UPDATE`.
- **Ekranlar (B–M)**: iskelet, sohbet, kişiler, ayarlar, yönetim, pano/belgeler, kütüphane, arama, overlay (`p2-overlay`
  pencere yöneticisi, `overlay-parity.mjs`), sohbet eksikleri (bağlantı/YouTube, PDF/video/ses/Office önizleme, `delete_any`,
  canlı biçimli yazım, sürükle-bırak, forum konu görünümü, iş parçacığı menüsü, bölünmüş görünüm ve ayrı pencere),
  kurulum/onboarding/açılış. Ayrıntılar ilgili kaynak dosyalarının başındaki yorumlarda.
- **Efektler (K)**: `third_party/gpui-pre` (+ `gpui-pre-windows` DirectX) yamaları — gerçek backdrop blur, 140 ms hover
  geçişleri, `scale/rotate/translate` dönüşümleri, Chromium tarzı kaydırma çubuğu, gri tonlama/parlaklık süzgeci, canlı yazı
  boyutu; yardımcılar `p2_ui::fx`. `P2_FX_OFF=1` eski davranış. Overlay'de hover/kaydırma efektleri kapalı.
- **Tam koşu (2026-11-13, 108 ekran × koyu/açık, ekran dışı pencerelerle)**: 215 karşılaştırmanın 123'ü ≤%2, 180'i ≤%3,
  201'i ≤%5. Kalan farkların çoğu metin kenar yumuşatması. Test pencereleri artık ekran dışında açılır (`P2_TEST_WINDOW_POS`,
  `gpui-pre-windows` yaması; Electron `P2_PARITY_WIN_X/Y`), kullanıcının masaüstü meşgul edilmez (`P2_PARITY_ONSCREEN=1` ile kapatılır).
- **Bilinen sınırlar**: overlay'de kalın yazı yok (`gpui-pre-wgpu/cosmic_text_system.rs` ağırlık yaması bekliyor) ve harf
  aralığı yok; overlay tarayıcı/sohbet pencereleri Electron'daki gömülü sayfa yerine "uygulamada/tarayıcıda aç" kartı; YouTube
  uygulama içinde oynatılmaz (tarayıcıda açılır); kutular döndürülemez (sürüklenen kartın 2,2° eğimi yok); uzak görsel önizlemesi
  Electron'da da görünmediği için kapalı (`media::REMOTE_IMAGE_PREVIEW`).
- **Electron'da bulunan hata**: `BoardPage.tsx` TaskComments emoji düğmesi `event.currentTarget`'ı durum güncelleyicisinin
  içinde okuyor → TypeError, uygulama boş ekrana düşüyor (dikdörtgen `setEmojiMenu`'den önce alınmalı).

---

## Ek A — Eşdeğerlik listesi (kabul ölçütü)

Her satır Rust sürümünde çalışır durumda işaretlenmeden Electron kaldırılmaz.

**Kabuk:** çerçevesiz pencere ve başlık çubuğu · büyüt/küçült/kapat ve kapatma davranışı · tepsi · tek örnek · `p2://` ve `--p2-play` · bildirimler ve tıklayınca açma · açılış ve güncelleme ekranı · popout pencereler · görev çubuğu simgesi/AppUserModelID · güncelleme bandı

**Hesap:** giriş · kayıt · 2FA (TOTP) · oturum hatırlama · çıkış · sunucu kurulumu/bağlanma/uzak sunucu · `ServerProblem` ekranı · şifre gücü göstergesi

**Sohbet:** kanallar ve kategoriler · kanal tarayıcısı · okunmamışlar · mesaj gönder/düzenle/sil · düzenleme geçmişi · tepkiler · bahsetme · emoji paneli/önerileri/kısa kodlar · Markdown ve kod renklendirme · bağlantılar ve video gömmeleri · ekler ve dosya bırakma · ek görüntüleyici (görsel/PDF/DOCX/arşiv) · yazıyor göstergesi · mesaj menüsü · DM'ler · gruplar · grup oluşturma

**Kişiler ve yönetim:** profil paneli · varlık durumu ve otomatik uzakta · arkadaşlık · yönetim paneli · ekip · erişim · roller · kanal izinleri · kullanıcı grupları · organizasyonlar ve davetler · proje ayarları

**Arama:** sesli arama · gelen arama ve zil · sustur/sağırlaştır · PTT · cihaz seçimi ve önizleme · ses ölçer · ekran paylaşımı ve kalite seviyeleri · sistem sesi paylaşımı · yayın oynatıcı ve sahne · arama widget'ı · ekran üstü çizim kaplaması · moderasyon · TURN/relay zorlama · arama istatistikleri

**Araçlar:** görev panosu, sprintler, kanban, yorumlar · belgeler: tuval ve sayfa, klasörler, izinler, canlı imleçler · proje notları · ekran görüntüleri · etkinlik · proje merkezi (gönderi/başarım/inceleme)

**Kütüphane:** liste · indir/kur/duraklat/sürdür/iptal · doğrula/onar · kaldır · başlat/durdur ve argümanlar · kanal seçimi · hız sınırı · kısayol · sürüm yükleme (sürdürülebilir) · sürüm farkı

**Overlay:** enjeksiyon (D3D11/D3D12) · kısayollar · pencere yöneticisi ve sabitleme · Sesli · Görevler · Notlar · Yakalama/ekran görüntüsü · Kütüphane · Genel Bakış · Performans · Sohbet · Ekip · Tarayıcı · overlay bildirimleri

**Görünüm:** P2 Mavi açık/koyu tema · vurgu rengi · 9 dil · 42 animasyonun her biri (`p2-fade`, `p2-rise`, `p2-page`, `p2-slide-*`, `p2-pop-elastic`, `p2-pop-out`, `p2-scale-in`, nabız/dönme/bar-sweep...) · "hareketi azalt" desteği

**Sunucu:** 173 REST ucu (sözleşme testleriyle) · 16 socket olayı · yüklemeler · sürüm deposu · güncelleme dağıtımı · sunucu konsolu · Windows servisi · Linux paketi · yedekleme · veritabanı migration'ları

## Ek B — Modül eşleme (TS → Rust)

| Bugün | Rust |
|---|---|
| `src/App.tsx`, `main.tsx`, `Titlebar` | `apps/p2-client` kabuk |
| `Workspace.tsx`, `ChannelContent.tsx`, `RichComposer.tsx`, `MessageRichText.tsx`, `messageMenu.tsx`, `EmojiPanel.tsx` | `p2-client::chat` |
| `DirectMessages.tsx`, `GroupChat.tsx`, `GroupCreator.tsx`, `ConversationMenu.tsx` | `p2-client::direct` |
| `SettingsModal.tsx`, `AppearanceSettings.tsx`, `settings.ts`, `hotkeys.ts` | `p2-client::settings` |
| `AdminPanel.tsx`, `AdminTeam.tsx`, `AdminAccess.tsx`, `ChannelPermissions.tsx`, `PermissionRow.tsx` | `p2-client::admin` + `p2-core::permissions` |
| `CallProvider.tsx`, `media.ts`, `systemAudio.ts`, `CallOverlay.tsx`, `StreamPlayer.tsx`, `StreamStage.tsx`, `ScreenShareModal.tsx`, `QualityMenu.tsx` | `p2-media` + `p2-client::call` |
| `BoardPage.tsx`, `api/tasks.ts`, `hooks/useProjectTasks.ts` | `p2-client::board` |
| `DocumentsPage.tsx`, `NotesEditor.tsx`, `ProjectNotesPanel.tsx` | `p2-client::documents` (+ editör) |
| `LibraryPage.tsx`, `LibraryUploader.tsx`, `ReleaseDiff.tsx`, `library.ts`, `electron/patcher*.cjs`, `cdc.cjs` | `p2-patcher`, `p2-cdc`, `p2-client::library` |
| `OverlayApp.tsx`, `src/overlay/**`, `overlayMirror.ts`, `overlayNotify.ts`, `electron/overlayHost.cjs` | `p2-client::overlay` + `p2-overlay-ipc` |
| `native/overlay/**` (C++) | `apps/p2-overlay-dll`, `apps/p2-inject` (Faz 7c) |
| `native/loopback` | `p2-media::loopback` |
| `theme.ts`, `p2-theme.css`, `ui.css`, `motion.css`, `electron/p2-mavi.cjs` | `p2-ui` |
| `src/locales/*` | `p2-i18n` (`.ftl`) |
| `electron/main.cjs`, `preload.cjs`, `updater.cjs` | `apps/p2-client` platform katmanı + güncelleyici |
| `electron/server-main.cjs`, `server-console.*`, `service/`, `server-updater.cjs` | `apps/p2-server-console`, `apps/p2-server` |
| `server/src/*.ts` | `apps/p2-server` (modül başına bir Rust modülü) |
| `server/prisma/schema.prisma` | `migrations/` (sqlx) — şema aynen |
