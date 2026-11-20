# Windows'ta P2 Server kurulumu

## Gereksinimler

- 64-bit Windows 10/11 veya Windows Server 2022+
- Yönetici yetkili kullanıcı
- PostgreSQL 15 veya daha yeni sürüm
- En az 2 GB RAM

## Adım adım

1. PostgreSQL'i kurun ve `postgres` kullanıcısına verdiğiniz parolayı saklayın.
2. `P2-Server-Setup-1.0.3.exe` dosyasını çalıştırıp P2 Server'ı kurun.
3. P2 Server Console'u açın.
4. PostgreSQL adresi olarak `127.0.0.1`, port olarak `5432`, kullanıcı olarak `postgres`, veritabanı olarak `p2` yazın.
5. PostgreSQL parolanızı girin; API portunu `4000` bırakın.
6. **Kaydet, tabloları hazırla ve başlat** düğmesine basın.
7. Durum “Çalışıyor” olduğunda **Windows servisi olarak kaydet** düğmesine basın ve yönetici iznini onaylayın.
8. İstemciyi kurup sunucu adresi olarak HTTPS alan adınızı girin.

Servis kurulduğunda P2 bilgisayar açılışında, kullanıcı oturumu açılmadan başlar. Konsolu kapatmak servisi durdurmaz.

Kurulum iki program içerir: **P2 Server** (Server Console, `P2 Server.exe`) ve API/servis ikilisi `p2-server.exe`. Node.js veya başka bir çalışma zamanı gerekmez; veritabanı göçleri ikilinin içindedir. Ayarlar (`server.json`), günlükler ve veriler `%ProgramData%\P2` altında tutulur.

## Yükseltme

Yeni `P2-Server-Setup-<sürüm>.exe` mevcut kurulumun üzerine çalıştırılır (ya da Server Console güncellemeyi kendisi indirir). Eski Node tabanlı kurulumlar da yerinde yükseltilir: aynı kurulum klasörü, kısayollar ve `%ProgramData%\P2` ayarları kullanılır; kayıtlı Windows servisi korunur ve bir sonraki başlatmada yeni `p2-server.exe` ile çalışır. Yükseltmeden önce **Yedek al** ile bir `.p2backup` üretin.

## HTTPS

İnternetten erişim için Caddy veya Nginx reverse proxy kullanın. Modemde yalnız `80` ve `443` portlarını yönlendirin. `4000` ve PostgreSQL `5432` portlarını internete açmayın.

```caddy
p2.ornek.com {
    reverse_proxy 127.0.0.1:4000
}
```

## Yedekleme

Server Console'daki **Yedek al** ile `.p2backup` üretin. **Yedek yükle** mevcut veritabanını değiştirir; geri yüklemeden önce ayrıca yedek alın.
