# P2 Patcher — Sürüm dağıtım ve oyun indirme (plan)

> **Not (0.9.9):** Bu plan uygulandı ve Rust'a taşındı. Bugünkü uygulama: istemci motoru `crates/p2-patcher`
> (içerik tanımlı parçalama, kurulum/güncelleme/onarım), sunucu `apps/p2-server/src/releases.rs`,
> `release_manifest.rs`, `release_store.rs`. Aşağıdaki Prisma modelleri artık `crates/p2-server-ops/migrations`
> SQL göçleriyle tanımlıdır; tasarım gerekçeleri için belge geçerliliğini korur.

Durum: **taslak plan**, kod yazılmadı. Sol raydaki yeni uygulama; Steam benzeri
bir "kütüphane + indirici" olarak proje sürümlerini (release) dağıtır.

## 1. Amaç ve kapsam

- Proje sahipleri/yetkililer bir **release** (sürüm) yükler: build klasörü ya da
  zip, sürüm numarası, kanal (stable / beta / nightly), sürüm notları.
- Ekip üyeleri raydaki **Kütüphane** uygulamasından projeyi seçer, release'i
  indirir, kurar/günceller ve oyunu başlatır. Sonraki sürümlerde yalnız değişen
  dosyalar iner (delta patch).
- Sunucu, dosyaları `%ProgramData%\P2\releases` (Linux: `/var/lib/p2/releases`)
  altında tutar; indirme istemcinin bağlı olduğu API adresinden yapılır (update
  dağıtımıyla aynı ilke: yayıncı makinesine bağımlılık yok).

Kapsam dışı (ilk sürümde yok): ödeme/lisans, mağaza vitrini, bulut kayıt, çok
kuruluşlu paylaşım, Mac/Linux istemci.

## 2. Veri modeli (Prisma)

```prisma
model Release {
  id          String   @id @default(uuid())
  projectId   String
  channel     String   @default("stable")   // stable | beta | nightly
  version     String                        // semver, proje+kanal içinde tekil
  title       String?
  notes       String?                       // markdown sürüm notları
  platform    String   @default("win-x64")
  executable  String                        // kök klasöre göre başlatılacak dosya
  sizeBytes   BigInt
  fileCount   Int
  state       String   @default("UPLOADING") // UPLOADING | READY | FAILED | ARCHIVED
  manifest    Json                          // [{path,size,sha256,chunks:[sha256...]}]
  createdById String
  createdAt   DateTime @default(now())
  publishedAt DateTime?
  project     Project  @relation(...)
  @@unique([projectId, channel, version])
  @@index([projectId, channel, publishedAt])
}
model ReleaseChunk {        // içerik adresli parça deposu; aynı parça bir kez saklanır
  hash      String  @id     // sha256
  size      Int
  refCount  Int     @default(0)
}
model ReleaseInstall {      // kimde hangi sürüm kurulu (telemetri + "güncelleme var" rozeti)
  userId, projectId, channel, version, deviceId, installedAt
}
```

Yetkiler (permissions.ts kataloğuna eklenir): `release.upload`, `release.publish`,
`release.delete`, `release.download` (varsayılan herkes açık), `release.channels.beta`
(beta/nightly görebilme).

## 3. Sunucu (NestJS) uçları

| Uç | Açıklama |
|---|---|
| `POST /projects/:id/releases` | Taslak release açar (metadata) |
| `POST /projects/:id/releases/:rid/manifest` | İstemci yerelde hesapladığı manifesti gönderir; sunucu eksik parça hash'lerini döner |
| `PUT /releases/:rid/chunks/:hash` | Eksik parçayı yükler (4 MB sabit boy, sha256 doğrulanır) |
| `POST /releases/:rid/publish` | state=READY, `release:published` olayı |
| `GET /projects/:id/releases?channel=` | Liste (yetkiye göre kanal filtresi) |
| `GET /releases/:rid/manifest` | Manifest (indirici bunu temel alır) |
| `GET /chunks/:hash` | Parça indirme (Range desteği, ETag=hash, önbelleklenebilir) |
| `POST /releases/:rid/installed` | Kurulum raporu |
| `DELETE /releases/:rid` | Arşivle; parçaların refCount'u düşer, 0 olanlar temizlenir |

Depolama: `releases/chunks/ab/cd/<hash>` düzeni. Yedekleme (`.p2backup`) bu
klasörü de kapsar; büyük olacağı için konsolda "yedeğe release dosyalarını dahil
et" seçeneği.

## 4. İstemci — Electron ana süreç (yeni `electron/patcher.cjs`)

Ağır iş renderer'da değil ana süreçte: hashleme, indirme, disk yazma.

- **Yükleyici:** klasör seçilir → dosyalar 4 MB parçalara bölünüp sha256'lanır
  (worker thread havuzu) → manifest → sunucuya "nelerin eksik" sorulur → yalnız
  eksik parçalar paralel yüklenir (4 eşzamanlı, yeniden deneme, kaldığı yerden).
- **İndirici:** hedef manifest ile yerel kurulumun manifesti (`.p2/manifest.json`)
  karşılaştırılır → değişen parçalar iner → dosyalar geçici isme yazılıp atomik
  taşınır → doğrulama → `manifest.json` güncellenir. Duraklat/sürdür, hız limiti,
  disk alanı kontrolü, "dosyaları doğrula/onar" komutu.
- **Kurulum dizini:** varsayılan `%LocalAppData%\P2\Games\<proje-slug>`; ayarlardan
  değiştirilebilir (farklı sürücü).
- **Başlatma:** `executable` yolu `child_process.spawn` ile; çalışıyorken "Oyunda"
  durumu presence'a yansır (isteğe bağlı), kapanınca rozet düşer.
- IPC köprüsü (`preload.cjs` → `window.p2.patcher`): `list`, `install`, `pause`,
  `resume`, `verify`, `uninstall`, `launch`, `upload`, `onProgress`.

## 5. İstemci — Arayüz (`src/LibraryPage.tsx`, `src/library.css`)

- Rayda yeni düğme (görevler/dokümanların altında): **Kütüphane**. Seçili projeye
  bağlı; proje değişince içerik değişir (pano/doküman sayfalarıyla aynı kalıp).
- Sol: projenin kanalları (stable/beta…) ve sürüm listesi; sağ: seçili sürümün
  notları, boyut, yayın tarihi, "İndir / Güncelle / Oyna / Doğrula" ana düğmesi,
  ilerleme çubuğu (hız, kalan süre).
- Yetkili için "Sürüm yükle" paneli: klasör seç, sürüm/kanal/notlar, hash +
  yükleme ilerlemesi, yayınla.
- Güncelleme rozeti: kurulu sürüm < yayınlanan en son sürüm ise ray düğmesinde
  rozet; `release:published` olayıyla canlı.
- Ayarlar > Kütüphane: kurulum klasörü, eşzamanlı indirme sayısı, hız limiti,
  "oyunu başlatınca P2'yi küçült".

## 6. Aşamalar

1. **Temel (1. sprint):** Prisma modelleri + migration, yetkiler, release CRUD,
   parça yükleme/indirme uçları, ana süreç indirici (duraklat yok), kütüphane
   sayfası (liste + indir + oyna). Tek platform: Windows x64.
2. **Delta + dayanıklılık:** manifest farkı, kaldığı yerden sürdürme, doğrula/onar,
   hız limiti, disk alanı uyarısı, arşivleme ve çöp toplama.
3. **Cilalama:** sürüm notları markdown, beta kanalı yetkisi, kurulum telemetrisi,
   yedek entegrasyonu, Linux sunucu paketinde `releases` dizini.

## 7. Açık kararlar (senin onayın gerekli)

- Parça boyu 4 MB ve içerik adresli depo: küçük sık güncellemeler için verimli,
  tamamen yeni build'lerde kazanç yok. Uygun mu?
- Kurulum klasörü kullanıcı başına (`%LocalAppData%`) mı, makine başına
  (`%ProgramData%`) mı? Yönetici izni istememek için kullanıcı başına öneriyorum.
- İlk sürümde yalnız Windows build'leri mi, yoksa platform alanı baştan çok
  platformlu mu kullanılsın?
- Steam'deki gibi "otomatik güncelle" varsayılan açık mı olsun?

## 8. Riskler

- Büyük yüklemelerde sunucu diski; yedek dosyası boyutu.
- Antivirüs/SmartScreen: indirilen exe'ler imzasız olabilir; kullanıcıya uyarı metni.
- OneDrive gibi eşitlenen klasörlere kurulum yapılmamalı (yol denetimi).
