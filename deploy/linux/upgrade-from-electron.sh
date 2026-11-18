#!/usr/bin/env bash
# P2 Server: Electron/Node kurulumundan Rust sunucusuna canlı yükseltme (Ubuntu, systemd + Caddy).
#
#   sudo bash upgrade-from-electron.sh
#
# Aynı klasörde olması gerekenler:
#   P2-Server-Ubuntu-24.04-x64-<v>.tar.gz (+ .sha256)   Rust sunucu paketi (install.sh içerir)
#   P2-Client-Setup-<v>.exe + client-latest.yml          Windows istemci kurulumu
#   P2-Server-Setup-<v>.exe + server-latest.yml          Windows sunucu kurulumu
#
# Sıra: 1) tam yedek (veritabanı pg_dump + /opt, /etc, /var/lib, servis, Caddyfile, indirme sitesi)
#       2) eski Node uygulaması silinmez, /opt/p2-server-electron-<zaman>'a taşınır
#       3) Rust paketinin install.sh'ı (ortam dosyası, veritabanı, JWT anahtarları korunur)
#       4) /health + veritabanı yanıtı denetlenir; olmazsa otomatik geri dönüş (rollback.sh)
#       5) indirme dosyaları (/downloads) ve güncelleme klasörü (/updates) yenilenir
set -euo pipefail

if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
  echo "root yetkisi gerekiyor: sudo bash $0" >&2
  exit 1
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSION="$(cat "$HERE/VERSION")"
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP="/var/backups/p2-upgrade-$STAMP"
ENV_FILE=/etc/p2-server/p2-server.env
DATA_DIR=/var/lib/p2-server
SITE=/var/www/p2.tdagaming.net
OLD_DIR="/opt/p2-server-electron-$STAMP"
SUDO_USER_NAME="${SUDO_USER:-root}"

log() { printf '\n== %s\n' "$*"; }

log "P2 Server $VERSION yükseltmesi ($STAMP)"
cd "$HERE"
sha256sum -c "P2-Server-Ubuntu-24.04-x64-$VERSION.tar.gz.sha256"
[[ -f "$ENV_FILE" ]] || { echo "$ENV_FILE yok — bu betik mevcut kurulumu yükseltmek içindir." >&2; exit 1; }

# DATABASE_URL'den veritabanı adı (postgresql://kullanıcı:parola@host:port/ad?...)
DATABASE_URL="$(grep -E '^DATABASE_URL=' "$ENV_FILE" | head -1 | cut -d= -f2- | sed -e 's/^"//' -e 's/"$//')"
DB_NAME="$(printf '%s' "$DATABASE_URL" | sed -E 's#^[^/]*//[^/]*/([^?]*).*#\1#')"
[[ -n "$DB_NAME" ]] || { echo "Veritabanı adı okunamadı." >&2; exit 1; }

log "1/5 Yedek: $BACKUP"
install -d -m 0700 "$BACKUP"
WAS_ACTIVE=0
if systemctl is-active --quiet p2-server.service; then
  WAS_ACTIVE=1
  echo "Tutarlı yedek için P2 Server durduruluyor (kısa kesinti)…"
  systemctl stop p2-server.service
fi
restart_old_on_error() {
  echo "HATA: yükseltme tamamlanmadı." >&2
  if [[ ! -x /opt/p2-server/p2-server && "$WAS_ACTIVE" == 1 ]]; then
    systemctl start p2-server.service || true
  fi
}
trap restart_old_on_error ERR

runuser -u postgres -- pg_dump -Fc "$DB_NAME" >"$BACKUP/db-$DB_NAME.dump"
echo "veritabanı: $(du -h "$BACKUP/db-$DB_NAME.dump" | cut -f1)"
tar -C / -czf "$BACKUP/files.tar.gz" \
  opt/p2-server etc/p2-server etc/systemd/system/p2-server.service etc/caddy/Caddyfile \
  "${SITE#/}" "${DATA_DIR#/}"
echo "dosyalar: $(du -h "$BACKUP/files.tar.gz" | cut -f1)"
cp "$HERE/rollback.sh" "$BACKUP/rollback.sh"
printf '%s\n' "$STAMP" >"$BACKUP/STAMP"
printf '%s\n' "$DB_NAME" >"$BACKUP/DB_NAME"
# İndirilebilir kopya (yalnız komutu çalıştıran kullanıcı okuyabilir)
if [[ "$SUDO_USER_NAME" != root ]]; then
  USER_HOME="$(getent passwd "$SUDO_USER_NAME" | cut -d: -f6)"
  install -d -o "$SUDO_USER_NAME" -m 0700 "$USER_HOME/p2-backups"
  cp -r "$BACKUP" "$USER_HOME/p2-backups/"
  chown -R "$SUDO_USER_NAME": "$USER_HOME/p2-backups"
fi

log "2/5 Eski Node uygulaması kenara: $OLD_DIR"
install -d -m 0755 "$OLD_DIR"
for d in app runtime; do
  [[ -e "/opt/p2-server/$d" ]] && mv "/opt/p2-server/$d" "$OLD_DIR/"
done
cp /etc/systemd/system/p2-server.service "$OLD_DIR/p2-server.service"

log "3/5 Rust sunucusu kuruluyor"
WORK="$(mktemp -d)"
tar -C "$WORK" -xzf "P2-Server-Ubuntu-24.04-x64-$VERSION.tar.gz"
PKG="$(dirname "$(find "$WORK" -name install.sh | head -1)")"
bash "$PKG/install.sh"
rm -rf "$WORK"

log "4/5 Denetim"
ok=0
for i in $(seq 1 60); do
  body="$(curl -fsS --max-time 3 http://127.0.0.1:4000/health 2>/dev/null || true)"
  if printf '%s' "$body" | grep -q '"status":"ok"'; then ok=1; break; fi
  sleep 1
done
setup="$(curl -fsS --max-time 5 http://127.0.0.1:4000/setup/status 2>/dev/null || true)"
echo "health: ${body:-yanıt yok}"
echo "setup : ${setup:-yanıt yok}"
if [[ "$ok" != 1 ]] || ! printf '%s' "$setup" | grep -q '"installed":true'; then
  echo "Rust sunucusu sağlıklı başlamadı — eski sürüme dönülüyor." >&2
  journalctl -u p2-server.service -n 60 --no-pager >&2 || true
  trap - ERR
  bash "$BACKUP/rollback.sh" "$BACKUP"
  exit 1
fi

log "5/5 İndirme ve güncelleme dosyaları"
install -d -o caddy -g caddy -m 0755 "$SITE/downloads"
install -o caddy -g caddy -m 0644 "P2-Server-Setup-$VERSION.exe" "$SITE/downloads/"
install -o caddy -g caddy -m 0644 "P2-Server-Ubuntu-24.04-x64-$VERSION.tar.gz" "$SITE/downloads/"
(cd "$SITE/downloads" && sha256sum "P2-Server-Setup-$VERSION.exe" >"P2-Server-Setup-$VERSION.exe.sha256" \
  && cp "$HERE/P2-Server-Ubuntu-24.04-x64-$VERSION.tar.gz.sha256" . && chown caddy:caddy ./*.sha256)
UPD="$(grep -E '^P2_UPDATE_DIR=' "$ENV_FILE" | tail -1 | cut -d= -f2- | sed -e 's/^"//' -e 's/"$//')"
UPD="${UPD:-$DATA_DIR/updates}"
echo "güncelleme klasörü: $UPD"
install -d -o p2server -g p2server -m 0750 "$UPD" "$UPD/client" "$UPD/server"
# Önce kurulum dosyaları, en son latest.yml (yarım yayın ilan edilmez); eski .blockmap'ler silinir.
install -o p2server -g p2server -m 0640 "P2-Client-Setup-$VERSION.exe" "$UPD/client/"
install -o p2server -g p2server -m 0640 "P2-Client-Setup-$VERSION.exe" "$UPD/"
install -o p2server -g p2server -m 0640 "P2-Server-Setup-$VERSION.exe" "$UPD/server/"
find "$UPD" -maxdepth 2 -name '*.blockmap' -delete
install -o p2server -g p2server -m 0640 client-latest.yml "$UPD/client/latest.yml"
install -o p2server -g p2server -m 0640 client-latest.yml "$UPD/latest.yml"
install -o p2server -g p2server -m 0640 server-latest.yml "$UPD/server/latest.yml"

trap - ERR
log "TAMAM — P2 Server $VERSION (Rust) çalışıyor"
echo "Yedek          : $BACKUP  (kopya: ~$SUDO_USER_NAME/p2-backups)"
echo "Eski uygulama  : $OLD_DIR"
echo "Geri dönmek    : sudo bash $BACKUP/rollback.sh $BACKUP"
