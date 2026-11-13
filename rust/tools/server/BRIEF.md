# Faz 2 — NestJS → Rust sunucu taşıma kuralları (alt ajanlar için ortak)

Hedef: `server/src/*.ts` (NestJS + Prisma) modüllerini `rust/apps/p2-server/src/*.rs` (axum 0.8 +
socketioxide 0.18 + sqlx 0.9, PostgreSQL) içine **birebir davranışla** taşımak. Aynı veritabanı şeması
(`server/prisma/schema.prisma`), aynı uçlar, aynı durum kodları, aynı JSON (anahtar sırası dahil), aynı
hata gövdeleri, aynı doğrulama, aynı yan etkiler (DB yazımları, gerçek zamanlı olay adları ve yükleri).
Mevcut Rust istemcisi ve Electron istemcisi bu sunucuyla değişiklik olmadan çalışmalı.

## Önce oku
- `rust/apps/p2-server/src/ctx.rs` — `Me` (kimlik çıkarıcı, AuthGuard), `Body` (JSON gövde), `Reply`/`ok`/`created`/`empty`,
  `uuid()`, `now()`, `iso!` makrosu, `sql()` (çalışma anı SQL'i), `page_args` (paging.ts), `Query`.
- `perm.rs` — permissions.ts'in tamamı (effective, require, require_channel, can_view_project, require_project_access,
  visible_channel_ids, channel_audience, project_audience, clean/clean_value/clean_channel, catalog_json, EVERYONE_DEFAULTS).
- `activity.rs` — json_build_object ile Prisma çıktısı üretme örneği (`ACTIVITY_JSON`), `record` (recordActivity),
  `mentioned_user_ids`, `preview_of`, `membership`. Uçların nasıl yazılıp `routes()` ile verildiğine bak.
- `realtime.rs` sonundaki yayın yardımcıları: `emit_users`, `emit_org` (gateway.emit), `broadcast`, `emit_channel`, `emit_project`.
- `calls.rs` — `invite_to_group_call`, `drop_from_group_call` (groups.ts bunları çağırır).
- `error.rs` (ApiError: new/unauthorized/bad_request/conflict/field/safe; NestJS gövde biçimi), `validate.rs` (Checker,
  js_len, js_slice, js_number, is_email, opt_str), `auth.rs` (DTO doğrulama ve uç örnekleri).
- `server/src/validation.ts` (Dto + validationException), `server/src/paging.ts`.

## Kurallar
- Uç dosyası `pub fn routes() -> Router<AppState>` verir (main.rs zaten hepsini merge ediyor). Yol parametreleri `{name}`.
- NestJS: `@Post` → 201 (`created`), diğerleri 200; `@HttpCode` varsa ona uy. `undefined` dönüş → boş gövde.
- Prisma JSON: model alanları şemadaki sırayla, ardından `include` ilişkileri include sırasıyla; `select` varsa select sırası.
  `json_build_object` kullan (json, jsonb DEĞİL — sıra korunur). Tarih: `iso!("col")` → `2026-11-12T19:33:00.123Z`; null kalır.
  Enum okurken `col::text`, yazarken `$n::"EnumAdı"`. `String[]` → `Vec<String>`. Int → JSON sayı. BigInt'e dikkat (Prisma BigInt JSON'da patlar; TS kodunun ne yaptığına bak).
- Zaman damgalarını (createdAt/updatedAt/`@default(now())`/`@updatedAt`) her zaman `ctx::now()` ile açıkça yaz: şemadaki
  CURRENT_TIMESTAMP varsayılanı yerel saati yazar (yanlış). Prisma `update` her seferinde `updatedAt`'i günceller.
- `$transaction` olan yerde sqlx transaction kullan. Prisma `findUniqueOrThrow`/`update` bulunamazsa 500'e düşer — TS'te ne oluyorsa aynısı.
- Hata metinleri ve kodlar TS'teki ile birebir (Türkçe). Yorumlar Türkçe, mevcut dosyaların üslubunda.
- Kullanıcı girdisi her zaman bind ile; `sql()` yalnız iç sabitlerden kurulan sorgu metni için.
- **Sahip olmadığın dosyalara dokunma**: main.rs, ctx.rs, perm.rs, activity.rs, realtime.rs, calls.rs, error.rs, validate.rs, auth.rs
  ve diğer ajanların dosyaları. Ortak bir yardımcı gerekiyorsa kendi dosyanda `pub` tanımla. Bağımlılık gerekirse
  `cargo add -p p2-server <crate>` (yalnız gerçekten gerekiyorsa).
- Derleme: `cd rust && cargo build -p p2-server --release --target-dir C:/p2-rust-target`. Diğer ajanlar aynı anda derliyor:
  kilit beklemesi normal; hata başka birinin dosyasındaysa 1-2 dk bekleyip yeniden dene. **Kendi dosyaların her an derlenir
  halde olsun** (büyük değişiklikleri derlenebilir adımlarla yap). Bash'te `cat > dosya` yazıp heredoc vermemek stdin'de takılır.
- Ortam: Node var (`node`). Test PostgreSQL :55432 ve NestJS :4110 çalışıyor — **durdurma/yeniden başlatma**. Kullanıcının gerçek
  sunucusuna/verisine dokunma; yalnız test veritabanı. Masaüstüne sentetik fare/klavye gönderme. Commit yok.

## Test (zorunlu)
`rust/tools/server/lib.mjs` (startRust(port), call, login, both, same, check, mask, done) ile `rust/tools/server/<modül>-test.mjs`
sözleşme testi yaz; örnek `activity-test.mjs`. Kendine ayrılan portu kullan (startRust(port) exe'yi kopyalayıp çalıştırır,
diğer ajanların sunucularını öldürme). Her uç için: başarılı yol, doğrulama hataları, yetki/bulunamadı yolları.
Salt okunur uçlarda `both(...)` ile aynı isteği iki sunucuya at. Yazan uçlarda aynı senaryoyu iki sunucuda ayrı taze
varlıklarla (rastgele etiketli adlar) çalıştır, maskeli cevapları karşılaştır; Rust'ın yazdığını NestJS ile okuyup da karşılaştır.
Gerçek zamanlı olayları `socket.io-client` ile (repo kökünde `createRequire(<repo>/package.json)`, bkz. scratchpad dışı örnek:
`/stream` namespace, `auth: { token }`, `subscribe {organizationId}`) dinleyip iki sunucuda aynı olay/yük geldiğini doğrula.
Test kullanıcıları: `p2test` (kuruluş sahibi), `rustbot` (üye), parola `Parola123`; kuruluş = `GET /organizations`[0].
Gerekirse yeni test kullanıcısı `/auth/register` ile açılabilir. İstemci uçtan uca testlerinin dayandığı mevcut veriyi silme;
kendi oluşturduğunu mümkünse temizle.

## Rapor
Taşınan uçların listesi, test sonucu (geçti/kaldı), bilinen sapmalar ve sebepleri, eklenen bağımlılıklar.
