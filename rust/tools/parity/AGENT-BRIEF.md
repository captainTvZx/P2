# Arayüz eşdeğerliği — ekran ajanları için ortak kurallar

Repo `C:\Users\capta\OneDrive\Desktop\P2` (dal `rust/faz-0`, commit YOK). Hedef: Rust/GPUI istemcisi
(`rust/apps/p2-client`, `rust/crates/p2-ui`) Electron/React istemcisiyle (`src/**`, `electron/**`)
**birebir aynı** görünmeli ve davranmalı — her metin, renk, font, boyut, boşluk, ikon, hover/active/focus
durumu, geçiş/animasyon, ipucu, modal ve sağ tık menüsü. Kullanıcı "farklı bir şey görmek istemiyorum" dedi.
Electron referanstır; Electron'da olmayan Rust öğeleri kaldırılır, Electron'da olup Rust'ta olmayan her
şey (menü maddeleri, düğmeler, paneller) eklenir ve çalışır hale getirilir.

## Önce oku
- `rust/tools/parity/README.md`, `TOKENS.md` (tasarım token'ları, GPUI geçici çözümleri), `catalog.json`
  (ekranlar, `owner` alanı), `docs/rust-gecis-plani.md` §9 notları.
- p2-ui ortak bileşenleri (ajan A): ikonlar `icons::icon(name).stroke(w).filled().el()`, `Icon::raw`,
  `svg_image`; bağlam menüsü görünümü için `screens/workspace_nav.rs` içindeki `nav_menu_view`
  (`.channel-context-menu`: cam zemin, 11px yarıçap, 32px satır).

## Döngü
`node rust/tools/parity/parity.mjs --screens <id,...> --themes dark,light` → yan yana/fark PNG'lerini
Read ile incele → `dom-styles.mjs` / `session.mjs` ile Electron'un hesaplanmış stillerini çıkar → düzelt →
tekrarla. Hedef: sahip olduğun her ekranda bölge farkı ~%2 altı ve gözle fark yok (metin kenar
yumuşatması gibi kaçınılmaz artıkları belgele). Katalogda olmayan durumlar (hover, açık menüler, modallar,
boş durumlar, hata durumları) için ekran ekle ve kanca yaz (`P2_AUTOPILOT_*` biçiminde, veri yazmayan).

## Dosya sahipliği
Yalnız kendine verilen dosyaları düzenle. Paylaşılan dosyaya (workspace.rs, chat.rs, app.rs, main.rs,
lib.rs, catalog.json) küçük kanca gerekiyorsa **düzenlemeden hemen önce yeniden oku**, küçük tut, raporda
belirt. A'nın p2-ui çekirdek dosyalarına (tokens, theme, palette, fonts, components, icons, icons_data,
lib) yazma; gerekirse kendi dosyanda yardımcı yaz ya da raporda iste. Başka bir ajanın yazdığı dosyanın
üzerine asla yazma (Write ile tüm dosyayı değil, Edit ile hedefli değişiklik).

## Kurallar
- Kullanıcının masaüstüne sentetik girdi yok (CDP yalnız düzeneğin açtığı Electron'a; PostMessage
  `p2-wincap` ile yalnız kendi Rust pencerelerine).
- **Gerçek ses G/Ç yok** (`P2_NO_AUDIO_DEVICE`; ses aygıtı açma — önceki bir deney Windows ses servisini çökertti).
- Kullanıcının gerçek profillerine/verisine/kurulu uygulamalarına dokunma; test PostgreSQL :55432 ve
  NestJS :4110'u durdurma.
- Derleme: `cd rust && cargo build --release --target-dir C:/p2-rust-target -p p2-client`. Başka ajanlar
  aynı anda derliyor: kilit beklemesi normal; kendi dosyaların her an derlenir olsun.
- Dokunduğun alanın işlev testlerini yeniden koş (`rust/tools/slice*-test.mjs <repo yolu>`, `SONUÇ`
  satırı; slice3 NestJS'te de zamanlama kaçırır).
- Kod yorumları Türkçe. Commit yok.

## Rapor
Ekran başına önce/sonra fark, eklenen/değişen dosyalar, paylaşılan dosyalardaki kancalar, kalan farklar ve
nedenleri, diğer ajanlara notlar.
