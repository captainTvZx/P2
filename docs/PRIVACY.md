# Gizlilik Politikası / Privacy Policy

Tam metin / Full text: <https://p2.tdagaming.net/docs/privacy/>

## Özet (Türkçe)

- P2 self-hosted'dır: veriler bağlandığınız P2 sunucusunda, o sunucuyu işletenin kontrolünde tutulur.
- İstemci ve sunucu telemetri, reklam ya da analitik verisi toplamaz; geliştiriciye kullanım verisi gönderilmez.
- Sunucuda: hesap bilgileri (parolalar argon2id özetiyle), oturum özetleri (IP adresi yok), mesajlar, dosyalar,
  görevler, belgeler, proje üyelikleri/rolleri/davetleri, durum ve kütüphane kurulum kayıtları.
- Bilgisayarda: `%APPDATA%\P2-Rust` ayarları, DPAPI ile şifreli oturum anahtarı, mesaj içeriği içermeyen tanı kaydı.
- Üçüncü taraf bağlantıları: aramalarda STUN (`stun.l.google.com`, yalnız IP/port görür), YouTube küçük resimleri
  (`i.ytimg.com`), yalnız Server Console'da genel IP sorgusu (`api.ipify.org`).
- Sesli/görüntülü arama ve ekran paylaşımı WebRTC ile uçtan uca şifreli iletilir, sunucuda kaydedilmez.

## Summary (English)

- P2 is self-hosted: data lives on the P2 server you connect to and is controlled by its operator.
- No telemetry, advertising or analytics; no usage data is sent to the developers; data is never sold.
- Server-side: account data (argon2id password hashes), session token hashes (no IP addresses), messages, files,
  tasks, documents, project membership/roles/invites, presence and library install records.
- Local: settings in `%APPDATA%\P2-Rust`, a DPAPI-encrypted session token, a diagnostic log without message contents.
- Third-party connections: STUN for calls (`stun.l.google.com`, sees IP/port only), YouTube thumbnails
  (`i.ytimg.com`), and a public-IP lookup (`api.ipify.org`) in the Server Console only.
- Calls and screen sharing use encrypted peer-to-peer WebRTC and are never recorded by the server.
