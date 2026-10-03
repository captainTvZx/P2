#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID:-$(id -u)} -ne 0 ]]; then
  echo "Bu kurulum root yetkisi ister: sudo bash install.sh" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR=/opt/p2-server
DATA_DIR=/var/lib/p2-server
CONFIG_DIR=/etc/p2-server
ENV_FILE="$CONFIG_DIR/p2-server.env"
SERVICE_WAS_ACTIVE=0

if systemctl is-active --quiet p2-server.service 2>/dev/null; then
  SERVICE_WAS_ACTIVE=1
  echo "Çalışan P2 Server hizmeti yükseltme için durduruluyor…"
  systemctl stop p2-server.service
fi

restart_previous_on_error() {
  if [[ "$SERVICE_WAS_ACTIVE" == 1 ]]; then
    echo "Kurulum başarısız oldu; önceki P2 Server yeniden başlatılıyor…" >&2
    systemctl start p2-server.service || true
  fi
}
trap restart_previous_on_error ERR

echo "P2 Server Ubuntu 24.04 kuruluyor…"
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends ca-certificates postgresql postgresql-client tar gzip
systemctl enable --now postgresql

if ! id p2server >/dev/null 2>&1; then
  useradd --system --home-dir "$DATA_DIR" --shell /usr/sbin/nologin p2server
fi
install -d -m 0755 "$INSTALL_DIR" "$INSTALL_DIR/app" "$INSTALL_DIR/runtime" "$INSTALL_DIR/app/tools"
install -d -o p2server -g p2server -m 0750 "$DATA_DIR" "$DATA_DIR/uploads" "$DATA_DIR/updates" "$DATA_DIR/backups"
install -d -m 0750 "$CONFIG_DIR"
cp -a "$ROOT/app/." "$INSTALL_DIR/app/"
tar -xJf "$ROOT/node-runtime.tar.xz" -C "$INSTALL_DIR/runtime" --strip-components=1
chmod 0755 "$INSTALL_DIR/runtime/bin/node" "$INSTALL_DIR/runtime/bin/npm" "$INSTALL_DIR/runtime/bin/npx" 2>/dev/null || true
chown -R root:root "$INSTALL_DIR"

if [[ ! -f "$ENV_FILE" ]]; then
  read -r -p "Bu makinedeki PostgreSQL kullanılsın mı? [E/h]: " LOCAL_DATABASE
  if [[ ! "$LOCAL_DATABASE" =~ ^[Hh]$ ]]; then
    read -r -s -p "P2 PostgreSQL parolası: " DB_PASSWORD
    echo
    [[ -n "$DB_PASSWORD" ]] || { echo "PostgreSQL parolası boş bırakılamaz." >&2; exit 1; }
    SQL_PASSWORD=${DB_PASSWORD//\'/\'\'}
    runuser -u postgres -- psql -v ON_ERROR_STOP=1 -c "DO \$\$ BEGIN IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'p2') THEN ALTER ROLE p2 WITH LOGIN PASSWORD '$SQL_PASSWORD'; ELSE CREATE ROLE p2 WITH LOGIN PASSWORD '$SQL_PASSWORD'; END IF; END \$\$;"
    if ! runuser -u postgres -- psql -tAc "SELECT 1 FROM pg_database WHERE datname='p2'" | grep -q 1; then
      runuser -u postgres -- createdb --owner=p2 --encoding=UTF8 p2
    fi
    ENCODED_PASSWORD="$($INSTALL_DIR/runtime/bin/node -e "process.stdout.write(encodeURIComponent(process.argv[1]))" "$DB_PASSWORD")"
    DATABASE_URL="postgresql://p2:$ENCODED_PASSWORD@127.0.0.1:5432/p2?schema=public"
  else
    read -r -p "PostgreSQL bağlantısı: " DATABASE_URL
    [[ -n "$DATABASE_URL" ]] || { echo "DATABASE_URL boş bırakılamaz." >&2; exit 1; }
  fi
  read -r -p "P2 API portu [4000]: " PORT
  PORT=${PORT:-4000}
  ACCESS_SECRET="$($INSTALL_DIR/runtime/bin/node -e "process.stdout.write(require('crypto').randomBytes(48).toString('hex'))")"
  REFRESH_SECRET="$($INSTALL_DIR/runtime/bin/node -e "process.stdout.write(require('crypto').randomBytes(48).toString('hex'))")"
  cat >"$ENV_FILE" <<EOF
DATABASE_URL="$DATABASE_URL"
JWT_ACCESS_SECRET="$ACCESS_SECRET"
JWT_REFRESH_SECRET="$REFRESH_SECRET"
HOST="0.0.0.0"
PORT="$PORT"
P2_SERVER_VERSION="__P2_VERSION__"
P2_UPLOAD_DIR="$DATA_DIR/uploads"
P2_UPDATE_DIR="$DATA_DIR/updates"
EOF
  chmod 0600 "$ENV_FILE"
else
  # Yükseltmede veritabanı/JWT/port ayarlarını koru; yalnız paket sürümünü yenile.
  if grep -q '^P2_SERVER_VERSION=' "$ENV_FILE"; then
    sed -i 's/^P2_SERVER_VERSION=.*/P2_SERVER_VERSION="__P2_VERSION__"/' "$ENV_FILE"
  else
    printf '\nP2_SERVER_VERSION="__P2_VERSION__"\n' >>"$ENV_FILE"
  fi
fi

install -m 0644 "$ROOT/p2-server.service" /etc/systemd/system/p2-server.service
install -m 0755 "$ROOT/p2-serverctl" /usr/local/sbin/p2-serverctl
systemctl daemon-reload
systemctl enable --now p2-server.service
trap - ERR
echo
echo "P2 Server kuruldu."
echo "Durum : sudo p2-serverctl status"
echo "Günlük: sudo p2-serverctl logs"
echo "Yedek : sudo p2-serverctl backup"
