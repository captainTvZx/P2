# Arkadaşlar ve Ana sayfa (projelerden bağımsız mesajlar)

Bu belge arkadaşlık sistemini ve birebir mesajların projelerden ayrılmasını anlatır: kararlar, veri
modeli, API, gerçek zamanlı olaylar ve istemci arayüzü. Proje toplulukları için bkz.
`docs/proje-bazli-yapi.md`.

## Kararlar

- **Arkadaşlık hesaba bağlıdır, projeye değil.** Bir projeden ayrılmak, çıkarılmak ya da projenin
  silinmesi arkadaşlığı etkilemez.
- **Birebir mesaj / arama kuralı:** iki kişi **arkadaşsa ya da en az bir projeyi paylaşıyorsa** ve
  **hiçbiri diğerini engellemediyse** mesajlaşabilir ve birbirini arayabilir.
  - Arkadaşlar için proje yetkisi (`direct.send`, `call.start`) aranmaz; arkadaşlık yeterlidir.
  - Arkadaş olmayanlar için eski kural sürer: ortak projelerden birinde gönderen yetkiyi taşımalıdır.
  - İkisi de yoksa `403 NO_SHARED_PROJECT`; engel varsa `403 USER_BLOCKED`. Eski mesajlar silinmez,
    okunabilir kalır.
- **Birebir sohbetler nerede yaşar?** Hepsi **Ana sayfada** (rayın en üstündeki düğme), hiçbiri bir
  projenin içinde değil. Proje kişi listesinden bir kişiye tıklamak ya da kişi menüsünden "Mesaj gönder"
  demek sohbeti **Ana sayfada** açar; proje kenar çubuğunda DM okunmamış rozeti gösterilmez (rozetler
  raydaki baloncuklardadır). Böylece proje sohbeti ile kişisel mesajlar hiç karışmaz.
- **Sohbet grupları** veri modelinde hâlâ projeye bağlıdır (üyeleri o projeden seçilir) ve projenin
  kenar çubuğundaki "Gruplar" bölümünde durur; Ana sayfadaki mesaj listesi ayrıca üye olunan **bütün**
  grupları da gösterir (`GET /organizations/:org/groups?all=1`).
- **Engel yönlüdür** (`UserBlock`): engelleyen ve engellenen birbirine mesaj atamaz, arayamaz, istek
  gönderemez; arkadaşlık ve bekleyen istek silinir. Engellenenin mesajları engelleyene gösterilmez
  (silinmez; engel kalkınca geri gelir), sohbet engelleyenin listesinde ve ray baloncuklarında görünmez.
  Engel kalkınca arkadaşlık geri gelmez; yeniden istek gerekir.
- **Çevrimiçi durum** artık arkadaşlara da gider (eskiden yalnız proje paylaşanlara).
- Sunucudan yasaklanan hesaplar (`ServerBan`) arkadaş listesinde ve istek aramasında görünmez.

## Veri modeli

Göç: `crates/p2-server-ops/migrations/20261117120000_friends`.

| Tablo | Açıklama |
| --- | --- |
| `Friendship` | İlk göçten beri vardı, kullanılmıyordu. Artık bir çift için **tek satır** (`LEAST/GREATEST` benzersiz dizini): `status` = `PENDING` (istek; `requesterId` gönderen) ya da `ACCEPTED`. `+ acceptedAt`. Eski `BLOCKED` satırları `UserBlock`'a taşınır. |
| `UserBlock` (yeni) | `blockerId, blockedId` (birincil anahtar), `createdAt`. |

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
| `GET /organizations/:org/direct` | Satıra `friend`, `blockedByMe`, `user.avatarUrl`, `user.title` eklendi; engellediğim kişinin mesajları okunmamış sayılmaz. |
| `POST /organizations/:org/direct` `{ userId }` | Arkadaş ya da ortak proje; yoksa `403 NO_SHARED_PROJECT`, engelde `403 USER_BLOCKED`. Var olan sohbet her zaman döner. |
| `GET .../direct/:id/messages` | Engellediğim kişinin mesajları dönmez. |
| `GET /organizations/:org/groups?all=1` | Üye olduğum bütün sohbet grupları. |

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

## İstemci

- `crates/p2-client-core/src/friends.rs`: `Friends`, `Friend`, `FriendEntry`, `Relation`, API çağrıları,
  hata kodu → çeviri anahtarı.
- `apps/p2-client/src/screens/friends_ui.rs`:
  - **Ray**: en üstte Ana sayfa düğmesi (okunmamış DM + bekleyen istek rozeti), altında okunmamış
    DM'lerin avatar baloncukları (en yeni önce, en çok 4), sonra ayırıcı ve projeler.
  - **Ana sayfa kenar çubuğu**: "Arkadaşlar" girişi, arama, mesaj listesi (birebir + gruplar; durum,
    son mesaj önizlemesi, okunmamış rozeti).
  - **Arkadaşlar sayfası**: Çevrimiçi / Tümü / Bekleyen / Engellenen sekmeleri ve "Arkadaş ekle" formu;
    satırlarda mesaj, arama, çıkar ve "diğer" menüsü.
  - **Kişi menüsü ve profil kartı** (her yerde): duruma göre Arkadaş ekle / İsteği iptal et / kabul et /
    Arkadaşlıktan çıkar, Engelle / Engeli kaldır, Mesaj gönder.
- Metinler `crates/p2-i18n/locales/{en,tr}.json` içinde `friends.*`.
- Görünüm kancası (test): `P2_AUTOPILOT_HOME=online|all|pending|blocked|add|dm:<kullanıcı>|menu:<kullanıcı>|profile:<kullanıcı>|request:<kullanıcı>|accept:…|block:…|unblock:…|remove:…|project:<sıra>` (`;` ile zincirlenir).

## Testler

- `tools/slice18-test.mjs` (sunucu): istek → kabul → ortak proje olmadan DM → projeden ayrıl, arkadaş
  kalır → engelle (DM, istek, arama yok; mesajlar gizli) → engeli kaldır → çıkar; olaylar ve durum yayını.
- `tools/slice18b-test.mjs` (istemci): Ana sayfa, sekmeler, arkadaş ekle, DM, kişi menüsü; koyu ve açık
  tema görüntüleri.

## Bilinen sınırlar

- Hiç projesi kalmayan kişi başlangıç (katıl / oluştur) ekranına döner; Ana sayfaya ulaşmak için en az
  bir projede olmak gerekir. Arkadaşlıklar ve mesajlar sunucuda durur, bir projeye girince geri gelir.
- Engel yalnız birebir alanı kapsar: ortak proje kanallarında ve gruplarda engellenen kişinin mesajları
  görünmeye devam eder.
