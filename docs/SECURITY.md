# P2 üretim güvenliği

- P2 API'yi Caddy veya Nginx üzerinden HTTPS ile yayınlayın.
- PostgreSQL'i yalnız `localhost` üzerinde dinletin; `5432` portunu internete açmayın.
- P2'nin `4000` portunu güvenlik duvarında dış dünyaya kapatın; yalnız reverse proxy erişsin.
- JWT ve veritabanı parolalarını kaynak depoya eklemeyin.
- Ubuntu'da `/etc/p2-server/p2-server.env` izinlerini `0600` olarak koruyun.
- Windows'ta `%ProgramData%\P2` klasörünü yalnız sunucu yöneticilerine açık tutun.
- İşletim sistemi, PostgreSQL, Caddy ve P2 güncellemelerini düzenli uygulayın.
- `.p2backup` dosyalarını şifreli ve sunucudan ayrı bir konumda saklayın.
- İnternete açık SSH/RDP erişiminde anahtar/VPN, güçlü kimlik doğrulama ve kaynak IP kısıtlaması kullanın.
