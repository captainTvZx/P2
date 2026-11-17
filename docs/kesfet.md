# Keşfet: herkese açık projeler ve haberleri

Her proje kendi topluluğudur (bkz. `docs/proje-bazli-yapi.md`). Keşfet, **herkese açık** projeleri ve
onların haberlerini tek ekranda gösterir; açık projeye davet kodu olmadan tek tıkla katılınır. Özel
projeler hiçbir Keşfet ucunda görünmez ve yalnız davet koduyla girilir.

## Model

Göç: `crates/p2-server-ops/migrations/20261117110000_project_visibility` (yalnız ekler).

| Tablo | Sütun | Açıklama |
| --- | --- | --- |
| `Project` | `visibility TEXT NOT NULL DEFAULT 'PRIVATE'` | `PUBLIC` \| `PRIVATE` (`CHECK`). Var olan projeler özel kalır. |
| `Project` | `category TEXT` | `GAME` \| `DEV` \| `COMMUNITY` \| `EDUCATION` \| `MUSIC` \| `OTHER`; boş = kategorisiz (listede `OTHER` sayılır). |
| `Project` | `tagline TEXT` | Kartta adın altındaki tek satır (en çok 120 karakter); yoksa açıklama gösterilir. |
| `ProjectPost` | `title TEXT`, `coverUrl TEXT` | Haberin isteğe bağlı başlığı ve kapağı. Başlık yoksa gönderinin ilk satırı, kapak yoksa projenin kapağı kullanılır. |

Proje nesnesi (`PROJECT_JSON`) artık `visibility`, `category`, `tagline` alanlarını da taşır.

## API

Hepsi oturum ister (`401`). `:id` = proje kimliği.

| Yöntem ve adres | Açıklama |
| --- | --- |
| `GET /discover/projects?q=&category=&sort=` | Yalnız `PUBLIC` projeler. `q`: ad / açıklama / tanıtım satırında büyük-küçük harf duyarsız arama (`%`, `_` düz karakter). `category`: yukarıdaki kimliklerden biri. `sort`: `popular` (üye sayısı, varsayılan) \| `new` \| `active` (çevrimiçi üye). Yanıt: `{ projects: [kart], categories: { GAME: 2, … }, total, online }` (kategori sayıları aramadan bağımsız; `online` açık projelerdeki tekil çevrimiçi kişi sayısı). |
| `GET /discover/projects/:id` | Kart + `owner`, `banned` (ben yasaklı mıyım), `channels` (kanal önizlemesi: `{ id, name, type, category }`; `channel.view` reddi olan kanallar hariç), `members` (en çok 14: `{ id, username, displayName, avatarUrl, online, owner }`), `news` (son 4 haber). Özel/silinmiş proje: `404 PROJECT_NOT_FOUND`. |
| `POST /discover/projects/:id/join` | Kodsuz katılma. Yanıt davetle katılmayla aynı: `{ project, organization, projectId, organizationId, joined }` (`joined=false` zaten üyeyse). Yasaklıysa `403 PROJECT_BANNED`; özel projede `404 PROJECT_NOT_FOUND`. |
| `GET /discover/news?limit=&projectId=` | Açık projelerin son gönderileri (en yeni önce, en çok 60): `{ id, projectId, kind, title, excerpt, content, coverUrl, ownCover, createdAt, author, project: { id, name, iconUrl, coverUrl, category, joined } }`. |
| `POST /projects`, `POST /organizations/:org/projects` | + `visibility?` (`PUBLIC`/`PRIVATE`, varsayılan `PRIVATE`), `category?`, `tagline?`. |
| `PATCH /projects/:id/admin` | + `visibility?`, `category?` (`''` kaldırır), `tagline?` (`''` kaldırır). "Projeyi düzenle" (`organization.manage`) ister. |
| `POST/PATCH …/hub/posts[/:postId]` | + `title?` (en çok 120), `coverUrl?` (`/uploads/...`). |

Kart: `{ id, name, slug, description, tagline, category, iconUrl, coverUrl, organizationId, visibility,
memberCount, onlineCount, postCount, joined, createdAt }`. `onlineCount`: açık bağlantısı olan ve görünmez
olmayan üyeler.

Kurallar:

- Görünürlük projeye aittir; özel yapılan proje listeden, önizlemeden, kodsuz katılmadan ve haberlerden
  hemen düşer. Var olan üyelikler etkilenmez.
- Yasaklar (`ProjectBan`) açık projede de geçerlidir.
- Önizleme e-posta gibi kişisel alan döndürmez; kanal listesinde herkese kapatılmış kanallar yer almaz.
- Proje açmak sunucu yetkisidir (`project.create`); görünürlük oluştururken ya da sonradan seçilir.

## İstemci

- `crates/p2-client-core/src/discover.rs`: `DiscoverProject`, `DiscoverList`, `DiscoverDetail`, `NewsItem`;
  `Api::discover_projects / discover_project / discover_join / discover_news`, `open_project_with`.
- `apps/p2-client/src/screens/discover.rs` (+ `discover_views.rs`, `discover_ui.rs`, `discover_images.rs`):
  `DiscoverView`. Tek ekran: solda arama + kategori çipli tanıtım alanı, öne çıkanlar (afiş kartları) ve tüm
  açık projelerin ızgarası (Popüler / Yeni / Etkin); sağda sürekli görünen "Haberler" sütunu (1120px'ten dar
  pencerede yatay şerit). Karta tıklayınca sağdan ayrıntı çekmecesi (afiş, açıklama, kanal önizlemesi, üyeler,
  son haberler, "Projeye katıl"); habere tıklayınca okuma penceresi. İskelet ve boş durumlar vardır.
- Erişim: başlangıç ekranındaki "Projelere göz at" kartı, proje rayındaki pusula düğmesi ve rayın "＋"
  menüsündeki "Projelere göz at" (`discover_host.rs`).
- Başlangıç ekranı (`onboarding.rs`): davet kodu alanı, "Projelere göz at" ve "Yeni proje oluştur" tek ekranda.
  `project.create` yetkisi yoksa oluşturma kartı soluk ve tıklanamazdır (ipucu: "Proje oluşturma yetkin yok —
  sunucu yöneticisinden iste").
- Başlangıç ekranındaki "Arkadaşlar ve mesajlar" satırı (kişinin kurulum kabı varsa; okunmamış birebir mesaj
  sayısıyla) çalışma alanını projesiz açar: Ana sayfa (arkadaşlar, DM), "＋" ve Keşfet raydan kullanılır
  (`onboarding::HOME_ONLY`; proje listesi boşken başlangıç ekranına geri dönülmez).
- Görünürlük: proje oluşturma (başlangıç formu, rayın "＋" penceresi) ve proje yönetim paneli › Ayarlar ›
  Keşfet (görünürlük, kategori, tanıtım satırı).
- Metinler `crates/p2-i18n` (`disc.*`, `vis.*`, `onb.*`; İngilizce + Türkçe; `tools/i18n-discover.mjs`).

## Otomatik pilot ve test

- `P2_AUTOPILOT_DISCOVER="adım;adım"`: Keşfet'i açar. Adımlar: `search:<metin>`, `category:<KİMLİK|all>`,
  `sort:<popular|new|active>`, `open:<ad>`, `join`, `news:<sıra>`, `back`, `scroll:<px>`, `close`
  (`P2_AUTOPILOT_DISCOVER_DELAY` ms).
- `P2_AUTOPILOT_ONBOARD=project|project-public|create:<ad>[:public[:KATEGORİ]]`, `P2_AUTOPILOT_PROJECT=<ad>`,
  `P2_AUTOPILOT_MANAGE=view:new-project-public`, `P2_AUTOPILOT_ADMIN=…;visibility:public|private`,
  `P2_TEST_WINDOW_SIZE=g,y`.
- `tools/discover-seed.mjs`: afişli örnek açık projeler, üyeler ve haberler (yalnız API; görseller yordamsal).
- `tools/slice17-test.mjs`: liste, arama, kategori, kodsuz katılma, özel proje listelenmez, yasaklı katılamaz,
  görünürlük aç/kapat, haberler + istemci akışları ve koyu/açık görüntüler (`--server-only` yalnız sunucu).
