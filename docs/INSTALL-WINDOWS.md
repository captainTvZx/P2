# Windows'ta P2 Server kurulumu

## Gereksinimler

- 64-bit Windows 10/11 veya Windows Server 2022+
- Yönetici yetkili kullanıcı
- PostgreSQL 15 veya daha yeni sürüm
- En az 2 GB RAM

## Adım adım

1. PostgreSQL'i kurun ve `postgres` kullanıcısına verdiğiniz parolayı saklayın.
2. `P2-Server-Setup-0.9.9.exe` dosyasını çalıştırıp P2 Server'ı kurun.
3. P2 Server Console'u açın.
4. PostgreSQL adresi olarak `127.0.0.1`, port olarak `5432`, kullanıcı olarak `postgres`, veritabanı olarak `p2` yazın.
5. PostgreSQL parolanızı girin; API portunu `4000` bırakın.
6. **Kaydet, tabloları hazırla ve başlat** düğmesine basın.
7. Durum “Çalışıyor” olduğunda **Windows servisi olarak kaydet** düğmesine basın ve yönetici iznini onaylayın.
8. İstemciyi kurup sunucu adresi olarak HTTPS alan adınızı girin.

Servis kurulduğunda P2 bilgisayar açılışında, kullanıcı oturumu açılmadan başlar. Konsolu kapatmak servisi durdurmaz.

## HTTPS

İnternetten erişim için Caddy veya Nginx reverse proxy kullanın. Modemde yalnız `80` ve `443` portlarını yönlendirin. `4000` ve PostgreSQL `5432` portlarını internete açmayın.

```caddy
p2.ornek.com {
    reverse_proxy 127.0.0.1:4000
}
```

## Yedekleme

Server Console'daki **Yedek al** ile `.p2backup` üretin. **Yedek yükle** mevcut veritabanını değiştirir; geri yüklemeden önce ayrıca yedek alın.
