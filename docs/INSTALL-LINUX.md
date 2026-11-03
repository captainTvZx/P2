# Ubuntu'da P2 Server kurulumu

## Gereksinimler

- Ubuntu Server 24.04 LTS veya daha yeni, x86-64
- `sudo` yetkili kullanıcı
- En az 2 GB RAM ve 10 GB disk alanı

## İndirme ve doğrulama

```bash
curl -LO https://p2.tdagaming.net/downloads/P2-Server-Ubuntu-24.04-x64-0.9.9.tar.gz
curl -LO https://p2.tdagaming.net/downloads/P2-Server-Ubuntu-24.04-x64-0.9.9.tar.gz.sha256
sha256sum -c P2-Server-Ubuntu-24.04-x64-0.9.9.tar.gz.sha256
```

## Kurulum

```bash
tar -xzf P2-Server-Ubuntu-24.04-x64-0.9.9.tar.gz
cd P2-Server-Ubuntu-24.04-x64-0.9.9
sudo bash install.sh
```

Yerel PostgreSQL sorusuna `E` yanıtı verin, güçlü bir veritabanı parolası belirleyin ve API portunu `4000` bırakın. Kurulum PostgreSQL'i, `p2server` sistem kullanıcısını ve açılışta başlayan `p2-server.service` hizmetini hazırlar.

## Yönetim

```bash
sudo p2-serverctl status
sudo p2-serverctl logs
sudo p2-serverctl restart
curl http://127.0.0.1:4000/health
```

## Caddy ile HTTPS

```caddy
p2.ornek.com {
    reverse_proxy 127.0.0.1:4000
}
```

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
```

`5432` ve `4000` portlarını internete açmayın; P2'yi HTTPS reverse proxy üzerinden yayınlayın.

## Yedekleme

```bash
sudo p2-serverctl backup
sudo p2-serverctl restore /yedekler/P2-Server-Backup.p2backup
```

Yedekleri düzenli olarak farklı bir diske veya şifreli uzak depoya kopyalayın.
