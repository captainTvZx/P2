# Arkadaşlar ve Ana sayfa (projelerden bağımsız mesajlar)

Bu belge arkadaşlık sistemini, Ana sayfayı ve birebir sohbetlerin iki kapsamını (Ana sayfa / proje)
anlatır: kararlar, veri modeli, API, gerçek zamanlı olaylar ve istemci arayüzü. Proje toplulukları için
bkz. `docs/proje-bazli-yapi.md`.

## Kararlar

- **Arkadaşlık hesaba bağlıdır, projeye değil.** Bir projeden ayrılmak, çıkarılmak ya da projenin
  silinmesi arkadaşlığı etkilemez.
- **İki ayrı birebir sohbet türü vardır.** Kullanıcının modeli: "bir projenin içinde kişiler arkadaşlardan
  ayrıdır; aynı kişiyle hem projenin içinden hem de arkadaş ekleyip direkt mesaj olarak konuşabilirim."
  - **Proje sohbeti:** projenin kişi listesinden ya da proje içindeki bir kişi kartı / menüsünden
    başlatılır. O projeye aittir, projenin kenar çubuğunda ("SOHBETLER" bölümü) durur ve yalnız **iki kişi
    de projenin üyesiyken** erişilir. Mesajları, okunmamışları ve yıldızları Ana sayfadan ayrıdır.
  - **Ana sayfa sohbeti (direkt mesaj):** hesaba bağlıdır; arkadaşlar (ya da Ana sayfadan başlatıldıysa
    proje paylaşan herkes) arasındadır. Yalnız Ana sayfada (rayın en üstündeki düğme) durur ve projeden
    ayrılınca da kalır.
  - Aynı iki kişinin **bir** Ana sayfa sohbeti ve paylaştıkları **her proje için birer** sohbeti olabilir;
    hepsi ayrıdır. (İlk tasarımda bütün birebir sohbetler Ana sayfaya taşınmıştı; bu karar onu değiştirir.)
- **Kural:**
  - *Proje sohbeti / proje araması:* iki kişi de projenin üyesi, gönderen o projede `direct.send`
    (arama için `call.start`) taşır, engel yok. Üye değilse `403 NOT_PROJECT_MEMBER`; biri projeden
    ayrılınca sohbet kapanır (`403 PROJECT_CONVERSATION_CLOSED`, listeden düşer; mesajlar silinmez, ikisi
    yeniden üye olunca geri gelir).
  - *Ana sayfa sohbeti / araması:* iki kişi **arkadaşsa ya da en az bir projeyi paylaşıyorsa** ve hiçbiri
    diğerini engellemediyse. Arkadaşlar için proje yetkisi aranmaz; arkadaş olmayanlar için ortak
    projelerden birinde gönderen yetkiyi taşımalıdır. İkisi de yoksa `403 NO_SHARED_PROJECT`; engel varsa
    `403 USER_BLOCKED`. Eski mesajlar silinmez, okunabilir kalır.
- **Kişiye tıklamak bulunduğun yere göre çalışır:** proje içinde (kişi listesi, kişi menüsü "Mesaj gönder",
  profil kartı, "Sesli ara", yıldız) o projenin kapsamında; Ana sayfada Ana sayfa kapsamında. Proje içindeki
  kişi menüsünde ayrıca "Arkadaş ekle" vardır.
- **Listeden kaldırma** kişi başınadır: sohbet yalnız kaldıranın listesinden çıkar, mesajlar silinmez.
  Yeni mesaj (ya da arama kaydı) gelince ya da kişi sohbeti yeniden açınca listeye geri döner. Kaldırırken
  sohbet okunmuş sayılır.
- **Yıldızlar kapsamlıdır:** Ana sayfada yıldızlanan kişi yalnız Ana sayfanın "YILDIZLI" bölümünde, bir
  projede yıldızlanan yalnız o projenin "YILDIZLI" bölümünde görünür.
- **Sohbet grupları** da iki türdür: **proje grubu** (üyeleri o projeden; projenin "Gruplar" bölümünde) ve
  **Ana sayfa grubu** (projeye bağlı değil; üyeler kuranın arkadaşları ya da proje paylaştığı kişiler;
  Ana sayfadaki mesaj listesinde). "Yeni mesaj" penceresinde birden çok kişi seçmek Ana sayfa grubu kurar.
- **Engel yönlüdür** (`UserBlock`): engelleyen ve engellenen birbirine mesaj atamaz, arayamaz, istek
  gönderemez; arkadaşlık ve bekleyen istek silinir. Engellenenin mesajları engelleyene gösterilmez
  (silinmez; engel kalkınca geri gelir), sohbet engelleyenin listesinde ve ray baloncuklarında görünmez.
  Engel kalkınca arkadaşlık geri gelmez; yeniden istek gerekir.
- **Çevrimiçi durum** artık arkadaşlara da gider (eskiden yalnız proje paylaşanlara).
- Sunucudan yasaklanan hesaplar (`ServerBan`) arkadaş listesinde ve istek aramasında görünmez.

## Veri modeli

Göçler: `crates/p2-server-ops/migrations/20261117120000_friends` (arkadaşlar, engeller) ve
`20261117130000_dm_scopes` (sohbet kapsamları, gizleme, Ana sayfa grupları, kapsamlı tercihler).

| Tablo | Açıklama |
| --- | --- |
| `Friendship` | İlk göçten beri vardı, kullanılmıyordu. Artık bir çift için **tek satır** (`LEAST/GREATEST` benzersiz dizini): `status` = `PENDING` (istek; `requesterId` gönderen) ya da `ACCEPTED`. `+ acceptedAt`. Eski `BLOCKED` satırları `UserBlock`'a taşınır. |
| `UserBlock` (yeni) | `blockerId, blockedId` (birincil anahtar), `createdAt`. |
| `DirectConversation` | `+ projectId` (NULL = Ana sayfa; dolu = proje sohbeti, `Project`'e `ON DELETE CASCADE`), `+ userAHidden`, `+ userBHidden` (kişi başına "listeden kaldır"). Çift anahtarı kapsamı içerir: `(organizationId, userAId, userBId) WHERE projectId IS NULL` ve `(projectId, userAId, userBId) WHERE projectId IS NOT NULL` kısmi benzersiz dizinleri. |
| `ChatGroup` | `+ home` (Ana sayfa grubu: `projectId` NULL, `home` = true). Ayrı sütun; eski yetim (projesiz) gruplar kendiliğinden Ana sayfada belirmesin diye. |
| `UserPreference` | `+ scope` (birincil anahtar artık `userId, kind, targetId, scope`). Kanal tercihlerinde boş; kişi tercihlerinde `home` ya da proje kimliği. |

**Var olan verinin taşınması** (`20261117130000_dm_scopes`):

- Var olan bütün birebir sohbetler **Ana sayfa sohbeti** olarak kalır (`projectId` NULL): Ana sayfadaki
  mevcut davranış ve geçmiş korunur; proje sohbetleri boş başlar ve ilk mesajla oluşur.
- Var olan gruplar proje grubu olarak kalır (`home` = false).
- Eski kişi tercihleri kapsamsızdı ve her projenin kenar çubuğunda görünüyordu: iki kişinin paylaştığı
  **her projeye** kopyalanır (proje kenar çubukları olduğu gibi kalır); ortak proje yoksa `home`
  kapsamına taşınır. Kanal tercihlerine dokunulmaz.

Bireysel sohbetler eskisi gibi `DirectConversation` (kuruluş = kurulumun hesap kabı) altındadır. Arkadaş
olan iki hesabın ortak kuruluşu yoksa (biri hiç projeye girmemişse) kabulde biri diğerinin kuruluşuna
`MEMBER` eklenir; kuruluş üyeliği hiçbir projeye erişim vermez.

## API

| Yöntem ve adres | Açıklama |
| --- | --- |
| `GET /friends` | `{ friends: [{ id, username, displayName, title, avatarUrl, status, since }], incoming: [{ id, user, createdAt }], outgoing: [...], blocked: [{ user, createdAt }] }`. Durum (`status`) yalnız arkadaşlarda döner. |
| `POST /friends/requests` `{ username \| userId }` | İstek gönderir → `201 { status: "PENDING", requestId, user }`. Karşı taraf zaten istek göndermişse doğrudan kabul edilir (`status: "ACCEPTED"`). Hatalar: `404 USER_NOT_FOUND`, `409 SELF_FRIEND`, `409 ALREADY_FRIENDS`, `409 FRIEND_REQUEST_EXISTS`, `409 BLOCKED_BY_ME`, `403 USER_BLOCKED`. |
| `POST /friends/requests/:id/accept` | Yalnız alıcı. `201 { status: "ACCEPTED", requestId, user }`. |
| `POST /friends/requests/:id/decline` | Yalnız alıcı; istek silinir. |
| `DELETE /friends/requests/:id` | Yalnız gönderen; istek iptal edilir. |
| `DELETE /friends/:userId` | Arkadaşlıktan çıkarır (`404 NOT_FRIENDS`). |
| `POST /blocks/:userId` | Engeller; arkadaşlık/istek silinir (`409 SELF_BLOCK`). |
| `DELETE /blocks/:userId` | Engeli kaldırır (`404 NOT_BLOCKED`). |
| `GET /organizations/:org/direct` | Bütün sohbetlerim: Ana sayfa + üyesi olduğum projelerdeki **açık** proje sohbetleri. Satırda `projectId` (NULL = Ana sayfa), `hidden`, `friend`, `blockedByMe`, `user.avatarUrl`, `user.title`, `unread` (sohbet başına ayrı). `?scope=home` yalnız Ana sayfa, `?projectId=<id>` yalnız o proje. |
| `POST /organizations/:org/direct` `{ userId, projectId? }` | `projectId` yoksa Ana sayfa sohbeti (arkadaş ya da ortak proje; yoksa `403 NO_SHARED_PROJECT`), varsa o projenin sohbeti (iki kişi de üye + `direct.send`; yoksa `403 NOT_PROJECT_MEMBER`). Engelde `403 USER_BLOCKED`. Var olan sohbet döner ve açan kişi için listeye geri gelir. |
| `PUT .../direct/:id/hidden` `{ hidden }` | Sohbeti kendi listemden kaldırır / geri getirir. Mesaj silinmez; okunmuş sayılır. |
| `GET .../direct/:id/messages`, `POST …/messages` | Engellediğim kişinin mesajları dönmez. Kapalı proje sohbetinde `403 PROJECT_CONVERSATION_CLOSED`. Gönderilen mesaj sohbeti iki tarafın listesine de geri getirir. |
| `GET /organizations/:org/groups?home=1` | Ana sayfa gruplarım. (`?projectId=` proje grupları; `?all=1` hepsi.) |
| `POST /organizations/:org/groups` `{ userIds, name?, projectId? }` | `projectId` yoksa **Ana sayfa grubu**: yalnız arkadaşlarım / proje paylaştıklarım eklenir (engel yoksa); hiçbiri uygun değilse `403 NO_SHARED_PROJECT`. |
| `PATCH /organizations/:org/groups/:id` `{ name }` | Grubu yeniden adlandırır (her üye; boş ad üyelerden türetilir). Sohbete sistem satırı düşer. |
| `PUT /organizations/:org/preferences` `{ kind, targetId, scope?, starred?, … }` | `kind: PERSON` için `scope` = `home` (varsayılan) ya da üyesi olduğum bir proje; `CHANNEL` için yok sayılır. `GET` satırlarında `scope` döner. |
| soket `call:start` `{ kind: "direct", targetUserId, projectId? }` | `projectId` varsa proje kuralı uygulanır ve arama kaydı o projenin sohbetine düşer; yoksa Ana sayfa. `call:state` / `call:incoming` yükündeki `call.projectId` kapsamı taşır. Ana sayfa gruplarında grup araması üyelik ile çalışır. |

Eski `GET /friends` (yalnız kabul edilmiş arkadaş dizisi; hiçbir istemci kullanmıyordu) kaldırıldı.

### Gerçek zamanlı olaylar

İki tarafın `user:` odalarına:

| Olay | Yük |
| --- | --- |
| `friend:request` | `{ requestId, from, to, createdAt }` |
| `friend:accepted` | `{ requestId, requester, addressee }` (+ iki tarafa birbirinin `presence:update`'i) |
| `friend:removed` | `{ requestId, userIds, byId, reason }` — `reason`: `declined`, `cancelled`, `removed` |
| `friend:blocked` | `{ byId, userId }` |
| `friend:unblocked` | `{ byId, userId }` (yalnız engeli kaldırana) |

`presence:update` / `presence:activity` arkadaşlara da gider (`perm::peers_among`).

Birebir olayların (`direct:created`, `direct:updated`, `direct:deleted`, `direct:reactions`,
`direct:conversation`) yükünde **`projectId`** bulunur (NULL = Ana sayfa): istemci olayı doğru listeye
yönlendirir. `direct:conversation` ayrıca `{ hidden }` ile yalnız ilgili kişiye gider (listeden kaldırma /
geri gelme; öteki oturumlar da güncellenir). Grup olaylarındaki `group` nesnesinde `home` vardır.

## İstemci

- `crates/p2-client-core/src/friends.rs`: `Friends`, `Friend`, `FriendEntry`, `Relation`, API çağrıları,
  hata kodu → çeviri anahtarı.
- `apps/p2-client/src/screens/friends_ui.rs`:
  - **Ray**: en üstte Ana sayfa düğmesi (okunmamış Ana sayfa sohbetleri + Ana sayfa grupları + bekleyen
    istek rozeti), altında okunmamış Ana sayfa sohbetlerinin avatar baloncukları (en yeni önce, en çok 4),
    sonra ayırıcı ve projeler. Proje sohbetlerinin okunmamışı o projenin ray rozetine eklenir.
  - **Ana sayfa kenar çubuğu**: "Arkadaşlar" girişi, arama, **"YILDIZLI"** bölümü (Ana sayfada yıldızlanan
    kişiler/sohbetler), "DİREKT MESAJLAR" listesi (yalnız Ana sayfa sohbetleri + Ana sayfa grupları; grup
    satırında üye avatarları ve grup simgesi). Başlıktaki **＋ "Yeni mesaj"** penceresini açar. Satıra sağ
    tık: kişi menüsü + **"Listeden kaldır"**; grup satırında "Sohbeti aç / Üyeler ve ad / Gruptan ayrıl".
  - **"Yeni mesaj" penceresi** (`new_message.rs`): arkadaşlar ve proje paylaşılan kişiler (arama, avatar,
    çevrimiçi durum, seçilenler çip olarak). Tek kişi → Ana sayfa sohbeti açılır/kurulur; birkaç kişi →
    Ana sayfa grubu (isteğe bağlı ad).
  - **Arkadaşlar sayfası**: Çevrimiçi / Tümü / Bekleyen / Engellenen sekmeleri ve "Arkadaş ekle" formu;
    satırlarda mesaj, arama, çıkar ve "diğer" menüsü.
  - **Kişi menüsü ve profil kartı** (her yerde): duruma göre Arkadaş ekle / İsteği iptal et / kabul et /
    Arkadaşlıktan çıkar, Engelle / Engeli kaldır, Mesaj gönder (bulunulan kapsamda), Sesli ara, yıldız.
- `workspace_nav.rs` — **proje kenar çubuğu**: "SOHBETLER" bölümü o projenin birebir sohbetlerini
  okunmamış rozetleriyle gösterir (sağ tık: "Listeden kaldır"); "KİŞİLER" listesinde bir kişiye tıklamak o
  projenin sohbetini açar (Ana sayfaya geçilmez); "YILDIZLI" yalnız o projede yıldızlananları gösterir.
- Sohbet başlığındaki "•••" menüsünde de "Listeden kaldır" ve (kapsamlı) yıldız vardır; grup üyeleri açılır
  listesinde "Grup adı" alanı ile yeniden adlandırma, Ana sayfa grubunda eklenecekler arkadaşlar ve proje
  arkadaşlarıdır.
- **Aramalar**: sohbetten başlatılan arama sohbetin kapsamını taşır (`CallController::start_direct(user,
  project)`). Bireysel ya da grup araması başlayınca (arayan için aradığı an, aranan için kabul edince)
  **arama görünümü kendiliğinden açılır** (`call_ui.rs` `call_ui_track`): ortada katılımcı kutucukları,
  süre / "çalıyor", sustur / sesi kapat / kamera / ekran / kişi ekle / kapat. Sesli odaların aksine bu
  aramaların ana alanda sahnesi yoktur; eskiden yalnız sol alttaki küçük panel çıkıyor, büyük görünüm
  hiçbir düğmeyle açılamıyordu. Paneldeki köşe düğmesi (ya da Esc) görünümü sol alttaki panele küçültür,
  aynı düğme yeniden büyütür. Panel çalışma alanı kökünde çizildiği için Ana sayfada da proje içinde de
  aynıdır; gelen arama kartı karşı tarafta nerede olursa olsun çıkar.
- Sunucu bireysel aramayı başlatmazsa (yetki yok, engel, proje üyesi değil) arayana `call:refused
  { code, targetUserId, projectId }` olayı gider ve istemci nedenini bildirir (eskiden sessizce hiçbir şey
  olmuyordu).
- Metinler `crates/p2-i18n/locales/{en,tr}.json` içinde `friends.*` ve `dm.*`.
- Görünüm kancası (test): `P2_AUTOPILOT_HOME=online|all|pending|blocked|add|dm:<kullanıcı>|pdm:<kullanıcı>|menu:<kullanıcı>|dmmenu:<kullanıcı>|hide:<kullanıcı>|star:<kullanıcı>|call:<kullanıcı>|expand|hangup|new|pick:<k1,k2>[|ad]|go|group:<ad>|groupmenu:<ad>|members|rename:<ad>|profile:<kullanıcı>|request:<kullanıcı>|accept:…|block:…|unblock:…|remove:…|project:<sıra>`
  (`;` ile zincirlenir). `dm:` her zaman Ana sayfa sohbetini, `pdm:` bulunulan kapsamın (proje içindeyse
  o projenin) sohbetini açar; `hide`, `star`, `call`, `dmmenu` bulunulan kapsamda çalışır.

## Testler

- `tools/slice18-test.mjs` (sunucu): istek → kabul → ortak proje olmadan DM → projeden ayrıl, arkadaş
  kalır → engelle (DM, istek, arama yok; mesajlar gizli) → engeli kaldır → çıkar; olaylar ve durum yayını.
  Ardından sohbet kapsamları: aynı kişiyle Ana sayfa sohbeti ≠ proje sohbeti (mesajlar, okunmamışlar,
  olaylarda `projectId`), listeden kaldır / yeni mesajla ve yeniden açınca geri gelme, kapsamlı yıldızlar,
  Ana sayfa grubu (kur, yaz, yeniden adlandır, üye ekle/çıkar, ayrıl, grup araması), aramanın kapsamı
  (arama kaydı doğru sohbete düşer), projeden ayrılınca proje sohbetinin kapanması.
- `tools/slice18b-test.mjs` (istemci, iki istemci + soket): proje kenar çubuğunda SOHBETLER ve rozetler,
  proje sohbeti (Ana sayfaya geçmeden), proje ve Ana sayfa sohbetinden arama (arama paneli, katılımcılar,
  karşı istemcide gelen arama kartı; ses aygıtı açılmaz), "Listeden kaldır" menüleri, kapsamlı yıldız,
  "Yeni mesaj" penceresi (tek kişi / grup), grup adı, Ana sayfa sekmeleri; koyu ve açık tema görüntüleri.

## Bilinen sınırlar

- Hiç projesi kalmayan kişi başlangıç (katıl / oluştur) ekranına döner; Ana sayfaya ulaşmak için en az
  bir projede olmak gerekir. Arkadaşlıklar ve mesajlar sunucuda durur, bir projeye girince geri gelir.
- Engel yalnız birebir alanı kapsar (Ana sayfa ve proje sohbetleri): ortak proje kanallarında ve gruplarda
  engellenen kişinin mesajları görünmeye devam eder.
- Gruplar yıldızlanamaz ve listeden kaldırılamaz (gruptan ayrılmak gerekir); yıldız yalnız kişiler içindir.
- Ana sayfa grubunda proje yetkisi yoktur: herkes yalnız kendi mesajını silebilir, üye çıkarmayı yalnız
  grubu kuran yapar.
