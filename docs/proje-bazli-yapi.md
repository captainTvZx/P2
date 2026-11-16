# Proje bazlı yapı: her proje kendi topluluğu

Bu belge P2'nin "her proje bir Discord sunucusu gibi kendi başına bir topluluktur" yapısına
geçişini anlatır: veri modeli, API, yetki kuralları, göç planı ve geriye uyumluluk. Faz 1
(bu belge) sunucuyu, veritabanı göçünü, API'yi ve `p2-client-core` tiplerini kapsar; istemci
arayüzü (katılma ekranı, proje yönetim paneli, kişi listesi) Faz 2'de bu API'nin üstüne kurulur.

## Kullanıcının kararları

- Her projeye **o projeye özel davet koduyla** katılınır; her projenin **kendi yönetim paneli** vardır.
- Bir davet kodu **yalnız o projeye** sokar. Kişi başka projeleri görmez; başka bir kod başka bir projeye sokar.
  Sunucu hesabı (kullanıcı) oluşturulur ya da var olan kullanılır ama ekip çapında hiçbir erişim vermez.
- Her şey projeye özgüdür: ayarlar, üye listesi, roller, kullanıcı grupları, kanal izinleri. Kuruluş
  (ekip) düzeyinde ortak bir şey yoktur.
- Var olan veri: kuruluş düzeyindeki roller, gruplar, yetkiler ve üyelikler **her projeye kopyalanır**;
  kimse erişim kaybetmez, sonra projeler birbirinden ayrışabilir.

## Model

### Kuruluş (Organization) — yalnız kap

`Organization` geriye uyumluluk için kalır: bir P2 kurulumunun (instance) hesap kabıdır. Bütün
`/organizations/:org/...` adresleri çalışmaya devam eder, ama **kuruluş üyeliği hiçbir projeye ya da
kişiye erişim vermez**.

- `OrganizationMember` satırı "bu hesap bu kurulumda" demektir. Proje davetiyle katılan kişiye,
  yoksa, `MEMBER` olarak eklenir. `permissions` / `deniedPermissions` sütunları artık okunmaz
  (göçte her projedeki `ProjectMember`'a kopyalanır).
- Kuruluş `OWNER`'ı = **kurulum sahibi**. Sunucu konsolu (yedek, geri yükleme, güncelleme) zaten
  makinede ve ona aittir. API'de yalnız şu kurulum uçları ona özeldir: kuruluş adını değiştirmek,
  bir kişinin bütün oturumlarını kapatmak, geri dönüşümdeki **her** projeyi görmek/kalıcı silmek.
  Kurulum sahibi projeleri **otomatik görmez**: göçte o güne kadar gördüğü her projeye `OWNER` olarak
  eklenir; sonradan başkasının açtığı projeyi ancak davetle görür.
- Eski kuruluş rolleri/grupları (`Role`/`UserGroup` satırları, `projectId` boş) silinmez, "arşiv"
  olarak kalır: yetki hesabında kullanılmaz, geri dönüş (rollback) için durur.

### Proje — topluluk

| Tablo | Değişiklik |
| --- | --- |
| `ProjectMember` | + `permissions TEXT[]`, `deniedPermissions TEXT[]` (kişiye özel aç/kapat), `joinedAt`, `inviteId`. `role`: `OWNER` = tam yetki (sahip); diğerleri yetkiyi rollerden alır. |
| `Role` | + `projectId` (Project'e bağlı, silinince gider), + `sourceRoleId` (göçte hangi kuruluş rolünden kopyalandığı). Benzersizlik: `projectId + handle` (proje rolleri), `organizationId + handle` yalnız arşiv satırlarında. |
| `UserGroup` | + `projectId`, + `sourceGroupId`; benzersizlik aynı şekilde. |
| `RoleMember`, `RoleGroup`, `UserGroupMember` | Değişmedi; artık proje rollerine/gruplarına bağlanır. |
| `ChannelPermission` | Değişmedi; ROLE/GROUP hedefleri kanalın projesinin rol/grup kimlikleridir. |
| `ProjectInvite` (yeni) | `id, code (benzersiz), projectId, createdById, expiresAt?, maxUses?, useCount, revoked, revokedAt?, createdAt` |
| `ProjectBan` (yeni) | `projectId, userId (birincil anahtar), bannedById, reason?, createdAt` |
| `OrganizationInvite` | + `revokedAt` (göçte bütün eski kodlar kapatılır, aşağıya bkz.) |

- **Üyelik** = projede `ProjectMember` satırı (ve proje silinmemiş). `Project.ownerId` "birincil
  sahip"tir: her zaman `OWNER` üyedir, çıkarılamaz/yasaklanamaz, sahipliği devredilebilir. Birden çok
  `OWNER` üye olabilir (göçte kurulum sahibi her projeye `OWNER` girer).
- **Yasak**: `ProjectBan` satırı. Yasaklanan kişinin üyeliği, rolleri, grup üyelikleri ve projedeki sohbet
  grubu üyelikleri silinir; yasak kalkana kadar hiçbir kodla geri katılamaz.
- **Proje açmak**: oturumu olan herkes açabilir (`project.create` yetkisi katalogdan kalktı). Açan kişi
  `OWNER` üye olur; projeye kurucu (`kurucu`, tüm yetkiler, üyeliği sahipliği izler) ve herkes
  (`everyone`, `EVERYONE_DEFAULTS`) rolleri, kuruluşa yapıldığı gibi tohumlanır, ve kalıcı bir davet kodu açılır.

## Yetki kuralları

`perm.rs` artık **proje başına** hesaplar: `effective_in(projectId, userId)`.

1. Kişi projenin üyesi değilse (ya da proje silinmişse) proje yok sayılır: `404 PROJECT_NOT_FOUND`.
2. `OWNER` üye → katalogdaki bütün yetkiler (eski kuruluş sahibi ayrıcalığının proje karşılığı).
3. Diğerleri → projenin `everyone` rolü ∪ doğrudan atanmış proje rolleri ∪ proje gruplarından gelen
   roller ∪ `ProjectMember.permissions`; sonra `ProjectMember.deniedPermissions` kapatılır.
4. Kanal kuralları eskisi gibi: everyone rolü → diğer roller → gruplar → kişi; her katmanda önce kapat, sonra aç.

Kuruluş düzeyinde kalan tek tük kontroller (proje dışı bağlam):

- **Bireysel mesaj / DM araması**: iki kişi en az bir projeyi paylaşmalı; gönderen `direct.send`'i
  (arama için `call.start`) paylaştıkları projelerden **en az birinde** taşımalı. Eski DM'ler silinmez,
  okunabilir kalır; ortak proje kalmayınca yeni mesaj/arama reddedilir (`NO_SHARED_PROJECT`).
- **DM mesajını süresiz silmek** (`message.delete_unlimited`): kişinin üye olduğu herhangi bir projede.
- **Sohbet grupları** zaten projeye bağlıdır: `group.create` o projede istenir, üyeler o projenin üyelerinden seçilir.

Görünürlük:

- Proje listesi, kanallar, panolar, dokümanlar, kütüphane, notlar, ekran görüntüleri: yalnız üye olunan projeler.
- Kişi listesi projeye özeldir (`/projects/:id/members`). Proje verilmeden istenen kuruluş kişi listesi
  "en az bir projeyi paylaştığım kişiler" olur (DM/kişi seçici için).
- Etkinlik (zil) listesi: projesi olan kayıtlar yalnız hâlâ üye olunan projelerden döner.
- Çevrimiçi/durum (`presence:update`, `presence:activity`) yalnız en az bir projeyi paylaşan kişilere
  ve kişinin kendisine gider. "Yazıyor" bildirimi kanalı görebilenlere gider (eskiden bütün kuruluşa).
- Rol/grup/kanal izni değişiklik olayları (`roles:changed`, `usergroup:changed`, `channel:permissions`,
  `channel:deleted`) yalnız o projenin üyelerine gider ve `projectId` taşır.

## API

`:p` = proje kimliği. Yeni kanonik adresler `/projects/:p/...` altındadır. Eski kuruluş adresleri
çalışır (aşağıdaki "Uyumluluk").

### Katılma ve davetler

| Yöntem ve adres | Açıklama |
| --- | --- |
| `GET /invites/:code` | **Oturum gerekmez.** Önizleme: `{ code, project: { id, name, description, iconUrl, coverUrl, memberCount }, expiresAt, valid, reason? }`. Geçersiz/bitmiş kod: `404 INVITE_NOT_FOUND` / `valid=false` + `reason` (`INVITE_EXPIRED`, `INVITE_FULL`, `INVITE_REVOKED`). |
| `POST /invites/:code/join` | Oturumlu kişi o projeye katılır. Yanıt: `{ project, organization, joined }` (`joined=false` zaten üyeyse). Yasaklıysa `403 PROJECT_BANNED`. |
| `POST /projects/join` `{ code }` | Aynısı, gövdeyle. |
| `POST /organizations/join` `{ code }` | **Uyumluluk.** Proje kodunu kabul eder; yanıt eskisi gibi kuruluş nesnesidir (+ `projectId`, `project`). Eski kuruluş kodları `400 INVITE_REVOKED`. |
| `POST /auth/register` `{ ..., inviteCode? }` | Kod verilirse önce doğrulanır (geçersizse hesap açılmaz), hesap açılınca projeye katılır. Yanıta `joined: { projectId, organizationId }` eklenir. |
| `GET /projects/:p/admin/invites[?includeRevoked=1]` | Etkin davetler (`member.invite`; iptal edilenler yalnız `includeRevoked` ile). Satır: `{ id, code, maxUses, useCount, expiresAt, createdAt, revoked, revokedAt, createdBy }` |
| `POST /projects/:p/admin/invites` `{ maxUses?, expiresInDays? }` | Yeni kod (`member.invite`). |
| `POST /projects/:p/admin/invites/rotate` | Süresiz/sınırsız varsayılan kodu yenisiyle değiştirir (eskisi iptal). |
| `DELETE /projects/:p/admin/invites/:inviteId` | Kodu iptal eder (`revoked=true`; satır kalır, sayılır). |

### Projeler

| Yöntem ve adres | Açıklama |
| --- | --- |
| `GET /projects` | Üye olduğum projeler (her kuruluştan), `{ ...proje, organizationId, role, owner }`. |
| `POST /projects` `{ name, slug, description?, organizationId? }` | Herkes proje açar. Kuruluş verilmezse kişinin ilk kuruluşu, o da yoksa kurulumun ilk kuruluşu (kişi `MEMBER` eklenir). |
| `POST /organizations/:org/projects` | Eskisi gibi; artık yetki istemez. |
| `GET /organizations/:org/projects` | Yalnız üye olunan projeler (kurulum sahibi dahil). |
| `GET /projects/:p/members` | Projenin kişi listesi: `{ id, username, displayName, email, title, avatarUrl, status, role, owner, joinedAt }` (üyeler görür). |
| `GET /organizations/:org/members?projectId=` | `projectId` ile proje listesi; onsuz paylaşılan kişiler. |
| `POST /projects/:p/leave` | Projeden ayrıl (birincil sahip ayrılamaz: önce devretmeli). |
| `GET /projects/:p/permissions/me` | `{ projectId, owner, permissions, roles, allow, deny, channels }` |
| `GET /organizations/:org/permissions/me[?projectId=]` | Proje verilirse o proje. Verilmezse: `permissions` = üye olunan projelerin birleşimi, `owner` = herhangi birinde sahip, `channels` = bütün projelerin kanalları (her kanal kendi projesine göre), ayrıca `projects: { [projectId]: { owner, permissions } }`. |

### Proje yönetim paneli — `/projects/:p/admin/...`

| Yöntem ve adres | Yetki | Açıklama |
| --- | --- | --- |
| `PATCH /projects/:p/admin` `{ name?, description?, iconUrl?, coverUrl?, defaultChannelId?, libraryEnabled? }` | `organization.manage` ("Projeyi düzenle"); kütüphane için ayrıca `release.publish` | Proje ayarları. |
| `GET /projects/:p/admin/overview` | `admin.panel` | `{ project, organization, server, members, content, invites, sessions, storage }` — sayılar projeye göre. |
| `GET /projects/:p/admin/members` | `admin.panel` | Üyeler + `role`, `primaryOwner`, rolleri, grupları, kanal mesaj sayısı. |
| `DELETE /projects/:p/admin/members/:u` | `member.kick` | Projeden çıkar (rol, grup, sohbet grubu üyelikleri silinir; tekrar davetle girebilir). |
| `GET /projects/:p/admin/bans` | `member.kick` | Yasaklılar `{ user, bannedBy, reason, createdAt }`. |
| `PUT /projects/:p/admin/bans/:u` `{ reason? }` | `member.kick` | Yasakla (üyeyse çıkarır). |
| `DELETE /projects/:p/admin/bans/:u` | `member.kick` | Yasağı kaldır. |
| `POST /projects/:p/admin/members/:u/transfer-owner` | birincil sahip | Projenin birincil sahipliğini devreder (kurucu rolü de geçer). |
| `PATCH /projects/:p/admin/members/:u/permissions` `{ allow, deny }` | `member.roles` | Kişiye özel yetkiler. |
| `GET /projects/:p/admin/permissions` | `member.roles` | Katalog + üyelerin etkin yetkileri + roller + gruplar. |
| `GET /projects/:p/admin/access` | `member.roles` | Üyeler × kanallar görünürlük tablosu. |
| `PATCH /projects/:p/admin/access/members/:u` `{ visible }` | `member.roles` (kapatmak `member.kick` gibi davranır) | Kişiyi projeye ekle/çıkar (eklemek için kişi benimle başka bir projeyi paylaşmalı ve yasaklı olmamalı). |
| `PATCH /projects/:p/admin/access/channels/:c/members/:u` `{ visible }` | `member.roles` | Kişiye kanal görünürlüğü. |
| `GET /projects/:p/admin/channels` | `admin.panel` | Projenin kanalları (silinenler dahil). |
| `GET /projects/:p/roles`, `POST`, `PATCH/DELETE /projects/:p/roles/:r`, `PATCH .../permissions`, `POST/DELETE .../members/:u`, `POST/DELETE .../groups/:g` | listeleme üye; değişiklik `member.roles` | Roller. Atanan kişi/grup projenin olmalı. |
| `GET /projects/:p/user-groups`, `POST`, `PATCH/DELETE .../:g`, `POST .../:g/members`, `DELETE .../:g/members/:u` | listeleme üye; değişiklik `usergroup.manage` | Kullanıcı grupları; yalnız proje üyeleri eklenir. |
| `GET/PUT/DELETE /organizations/:org/projects/:p/channels/:c/permissions[/:type/:target]` | `channel.permissions` | Kanal kuralları (adres aynı); roller/gruplar/kişiler artık yalnız bu projeninkiler. |

Aynı yönetim uçları `/organizations/:org/projects/:p/admin/...` altında da vardır.

### Kurulum (kuruluş sahibi) uçları — `/organizations/:org/admin/...`

| Adres | Davranış |
| --- | --- |
| `PATCH /organizations/:org/admin` `{ name }` | Kuruluş adı; yalnız kuruluş sahibi. |
| `POST /organizations/:org/admin/members/:u/sessions/revoke` | Kişinin **bütün** oturumlarını kapatır; yalnız kuruluş sahibi (proje yöneticisi başka projelerdeki oturumu kapatamaz). |
| `GET /organizations/:org/admin/projects/trash` | Geri dönüşüm: kuruluş sahibi hepsini, diğerleri yalnız `OWNER` olduğu projeleri görür. |
| `DELETE /organizations/:org/admin/projects/:p/purge` | Kalıcı silme: kuruluş sahibi ya da projenin `OWNER`'ı. |

## Uyumluluk

- Bütün `/organizations/:org/...` içerik adresleri (proje, kanal, mesaj, pano, doküman, kütüphane, DM,
  sohbet grubu, etkinlik, tercih) aynı kalır; yalnız yetki/görünürlük proje bazlı hesaplanır.
- Eski kuruluş yönetim adresleri (`/organizations/:org/admin/{overview,members,permissions,invites,access,channels,...}`,
  `/organizations/:org/roles...`, `/organizations/:org/user-groups...`) **proje kapsamına eşlenir**. Proje şu
  sırayla seçilir: `?projectId=` sorgu parametresi → `X-P2-Project` başlığı → (geçiş dönemi için) kişinin
  bu kuruluşta üye olduğu, en eski açılmış proje. Yeni istemci her zaman `/projects/:p/...` kullanmalı;
  varsayılan proje seçimi yalnız eski istemci sürümleri bozulmasın diye vardır.
- `GET /organizations` artık `inviteCode` döndürmez (kuruluş kodu yok); proje kodları proje panelindedir.
- Olaylar: `organization:member:joined` / `organization:member:removed` yalnız o projenin üyelerine,
  `projectId` alanıyla gider (yanında yeni adlar `project:member:joined` / `project:member:removed`).
  Çıkarılan/yasaklanan kişiye `project:removed { projectId, banned }` gider. `permissions:changed` olayı
  `projectId` taşır.

## Göç planı

Göç: `crates/p2-server-ops/migrations/20261116100000_project_communities`. Yalnız ekleme yapar; eski
göçler değişmez. Tek işlemde (BEGIN/COMMIT) çalışır; her adım tekrar çalıştırılsa da aynı sonucu verir
(`ON CONFLICT DO NOTHING`, `NOT EXISTS`, `sourceRoleId`/`sourceGroupId` eşlemesi).

1. Şema: yukarıdaki sütunlar, tablolar, indeksler. Rol/grup benzersizlik indeksleri kısmi indekslere çevrilir.
2. **Eski erişim kuralıyla üyelik** (`can_view_project` / `project_audience`): kuruluşu olan her proje için
   - kuruluş üyesi olmayanların bayat `ProjectMember` satırları silinir (eskiden `membership()` onları zaten
     dışarıda tutuyordu; yeni kuralda satır tek başına erişim vereceği için silinmeleri şart);
   - proje sahibi (`ownerId`) kuruluş üyesiyse `OWNER` üye olur;
   - kuruluş sahipleri her projeye `OWNER` üye olur (eski "kuruluş sahibi her şeyi görür" kuralı);
   - kalan üyeliklere kuruluştaki kişiye özel `permissions`/`deniedPermissions` kopyalanır.
3. **Rol ve grup kopyası**: her proje için kuruluşun her rolü (`projectId` boş) yeni kimlikle kopyalanır
   (`sourceRoleId` = eski kimlik); `RoleMember` yalnız proje üyeleri için kopyalanır. Gruplar da aynı şekilde
   (`sourceGroupId`), `UserGroupMember` yalnız proje üyeleri, `RoleGroup` yeni kimliklerle.
4. **Kanal kuralları**: projenin kanallarındaki `ROLE`/`GROUP` hedefleri kopya kimliklere çevrilir.
5. Kuruluşunda rol olmadığı için temel rolleri eksik kalan projeye `everyone` + `kurucu` tohumlanır
   (kurucu üyeleri = `OWNER` üyeler) ve her projeye kalıcı bir davet kodu açılır.
6. **Eski kuruluş davetleri kapatılır** (`revokedAt` doldurulur). Neden dönüştürülmedi: 2026-09-29'daki
   `project_visibility` göçünden beri kuruluş koduyla katılan kişi **hiçbir projeyi görmüyordu** (proje erişimi
   ayrı veriliyordu; `everyone` rolü proje görünürlüğü vermez). Kodları "everyone'ın görebildiği projelere"
   çevirmek boş kümeye çevirmek demek; bir projeye çevirmek ise eskiden olmayan bir erişimi açmak olurdu.
   Kapatmak erişim açısından hiçbir şeyi değiştirmez; ekip yöneticisi proje panelinden yeni kod üretir.

### Doğrulama

`tools/project-scope-verify.mjs`: göçten önce **eski** sunucu ikilisiyle, sonra **yeni** ikiliyle her kuruluş
üyesi için proje listesini ve kanal başına etkin yetkileri alır, karşılaştırır. Proje listesi farkı, kanal
kaybı ya da yetki kaybı varsa başarısız olur. Yalnız kimlik ve yetki adı yazar (kişisel veri yok).
Üretim kopyası üzerindeki sonuç rapordadır. Beklenen tek fark: kuruluş sahibi olmayan proje sahipleri
artık kendi projelerinde tam yetkilidir (yetki **kazanımı**, kayıp değil).

### Geri dönüş

Göç yalnız ekler; eski kuruluş rolleri, grupları ve üyelikleri yerinde durur. Eski ikiliye dönülürse
eski kod `organizationId` ile çalışır (yeni sütunları yok sayar). İki şey eski hâline dönmez: kanal
kurallarının ROLE/GROUP hedefleri kopya kimliklere çevrilmiştir (eski ikilide bu kurallar etkisiz kalır)
ve bayat `ProjectMember` satırları silinmiştir (eski ikilide de etkisizdiler). Tam geri dönüş için
güncelleme öncesi yedekten (`p2-server restore`) dönülür.

## Faz 2: istemci arayüzü

- **Katılma**: projesi olmayan kişi başlangıç ekranında davet kodunu girer → `GET /invites/:code` önizleme
  kartı (ad, ikon, açıklama, üye sayısı, geçerlilik) → `POST /projects/join`. Projesi olan kişi rayın "＋"
  menüsünden "Davet koduyla katıl" (aynı kart, `screens/join_project.rs`) ya da "Yeni proje oluştur"u seçer.
  Kayıt formunda isteğe bağlı davet kodu vardır (`register_with_invite`). "Kuruluş oluştur" yalnız ilk
  kurulumda (ilk yönetici) ya da sunucuda kuruluş yokken (`NO_ORGANIZATION`) görünür.
- **Proje yönetim paneli** (`screens/admin.rs` + `admin_views.rs`): proje başlığındaki ⋯ menüsünden ya da
  raydaki projenin sağ tık menüsünden "Proje ayarları / Yönetim". Bölümler: Genel, Üyeler (sahiplik devri,
  çıkar, yasakla), Roller, Gruplar, İzinler (kanal görünürlüğü + kişiye özel yetkiler), Davetler, Yasaklılar,
  Kanallar, Ayarlar (ad, açıklama, ikon, kapak, başlangıç kanalı, kütüphane). Hepsi `/projects/:p/...` uçlarıdır;
  sekmeler `GET /projects/:p/permissions/me` ile süzülür.
- **Sunucu yönetimi**: kullanıcı menüsünde yalnız kurulum sahibine (kuruluş `OWNER`) görünür; kuruluş adı,
  oturum kapatma ve projelerin geri dönüşümü/kalıcı silinmesi.
- **Projeden ayrıl**: proje ⋯ menüsü ve ray sağ tık menüsü; birincil sahip önce sahipliği devretmeye yönlendirilir.
- **Gerçek zamanlı**: `project:removed` (çıkarıldın/yasaklandın), `project:deleted`, `project:purged` projeyi
  raydan düşürür ve bildirim gösterir; `project:member:joined/removed` kişi listesini günceller.
- Kişi listesi `GET /projects/:p/members`, DM seçici `members(org)` (ortak projesi olanlar); @grup bahsetmesi
  `GET /projects/:p/user-groups`.
- Test: `tools/slice15-test.mjs` (istemci otomatik pilotuyla katılma ve yönetim paneli, koyu/açık görüntüler).
