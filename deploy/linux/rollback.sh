#!/usr/bin/env bash
# upgrade-from-electron.sh'ın geri dönüşü: Electron/Node P2 Server'ı, yükseltme öncesi veritabanı,
# ayar, veri ve indirme dosyalarıyla geri getirir.
#
#   sudo bash rollback.sh /var/backups/p2-upgrade-<zaman>
set -euo pipefail
[[ ${EUID:-$(id -u)} -eq 0 ]] || { echo "sudo bash $0 <yedek klasörü>" >&2; exit 1; }
BACKUP="${1:?yedek klasörü}"
STAMP="$(cat "$BACKUP/STAMP")"
DB_NAME="$(cat "$BACKUP/DB_NAME")"
OLD_DIR="/opt/p2-server-electron-$STAMP"

echo "== Geri dönüş: $BACKUP"
systemctl stop p2-server.service 2>/dev/null || true

# Uygulama: kenara alınan Node uygulaması (yoksa yedekten)
rm -f /opt/p2-server/p2-server
if [[ -d "$OLD_DIR/app" ]]; then
  for d in app runtime; do rm -rf "/opt/p2-server/$d"; mv "$OLD_DIR/$d" /opt/p2-server/; done
else
  tar -C / -xzf "$BACKUP/files.tar.gz" opt/p2-server
fi
# Ayar, servis, veri, site, Caddyfile: yedekteki hâli
rm -rf /etc/p2-server /var/lib/p2-server /var/www/p2.tdagaming.net
tar -C / -xzf "$BACKUP/files.tar.gz" etc/p2-server etc/systemd/system/p2-server.service etc/caddy/Caddyfile \
  var/www/p2.tdagaming.net var/lib/p2-server
rm -f /usr/local/sbin/p2-serverctl

# Veritabanı: yükseltme öncesi döküm
runuser -u postgres -- pg_restore --clean --if-exists --no-owner --role="$(runuser -u postgres -- psql -tAc "select pg_catalog.pg_get_userbyid(datdba) from pg_database where datname='$DB_NAME'")" -d "$DB_NAME" "$BACKUP/db-$DB_NAME.dump"

systemctl daemon-reload
systemctl start p2-server.service
sleep 3
curl -fsS --max-time 5 http://127.0.0.1:4000/health && echo
echo "== Electron/Node P2 Server geri getirildi."
