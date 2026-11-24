#!/usr/bin/env bash
# TURN sunucusu (coturn) kurulumu: iki kişi doğrudan bağlanamadığında (simetrik NAT / CGNAT) sesli ve
# görüntülü arama bu sunucu üzerinden aktarılır. P2 Server her aramada istemciye geçici TURN kimliği verir
# (coturn use-auth-secret); kullanıcıların ayarlara bir şey girmesi gerekmez.
#
#   sudo bash turn-setup.sh p2.ornek.com           (alan adı ya da genel IP)
#   sudo p2-serverctl turn-setup p2.ornek.com
#
# Modemde/güvenlik duvarında açılması gerekenler: 3478 UDP+TCP ve 49160-49260 UDP (aktarma portları).
set -euo pipefail
[[ ${EUID:-$(id -u)} -eq 0 ]] || { echo "Bu kurulum root yetkisi ister: sudo bash $0 <alan adı>" >&2; exit 1; }
HOST="${1:-}"
[[ -n "$HOST" ]] || { echo "Kullanım: sudo bash $0 <sunucunun alan adı ya da genel IP'si>" >&2; exit 2; }

ENV_FILE=/etc/p2-server/p2-server.env
DATA_DIR=/var/lib/p2-server
if [[ -f "$ENV_FILE" ]]; then
  d="$(grep -E '^P2_SERVER_DATA_DIR=' "$ENV_FILE" | head -1 | cut -d= -f2- | sed -e 's/^"//' -e 's/"$//')"
  [[ -n "$d" ]] && DATA_DIR="$d"
fi
INI="$DATA_DIR/p2-server.ini"

echo "coturn kuruluyor…"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq coturn >/dev/null

SECRET="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
CONF=/etc/turnserver.conf
[[ -f "$CONF" && ! -f "$CONF.p2-yedek" ]] && cp "$CONF" "$CONF.p2-yedek"
EXTERNAL=""
PUBLIC_IP="$(curl -fsS --max-time 5 https://api.ipify.org 2>/dev/null || true)"
LOCAL_IP="$(hostname -I 2>/dev/null | awk '{print $1}')"
if [[ -n "$PUBLIC_IP" && -n "$LOCAL_IP" && "$PUBLIC_IP" != "$LOCAL_IP" ]]; then
  # NAT arkasındaki sunucu: aktarma adresleri dışarıya genel IP ile bildirilir.
  EXTERNAL="external-ip=$PUBLIC_IP/$LOCAL_IP"
fi
cat >"$CONF" <<CONF
# P2 TURN (turn-setup.sh). Kimlik P2 Server'ın verdiği geçici kullanıcı/parola ile (use-auth-secret).
listening-port=3478
fingerprint
use-auth-secret
static-auth-secret=$SECRET
realm=$HOST
server-name=$HOST
min-port=49160
max-port=49260
$EXTERNAL
no-cli
no-tlsv1
no-tlsv1_1
no-multicast-peers
# Yerel ağlara aktarma yapılmaz (TURN üzerinden iç ağa erişim engellenir).
denied-peer-ip=10.0.0.0-10.255.255.255
denied-peer-ip=172.16.0.0-172.31.255.255
denied-peer-ip=192.168.0.0-192.168.255.255
denied-peer-ip=127.0.0.0-127.255.255.255
denied-peer-ip=169.254.0.0-169.254.255.255
denied-peer-ip=0.0.0.0-0.255.255.255
denied-peer-ip=100.64.0.0-100.127.255.255
denied-peer-ip=::1
CONF
chmod 0640 "$CONF"
chown root:turnserver "$CONF" 2>/dev/null || true
[[ -f /etc/default/coturn ]] && sed -i 's/^#\?TURNSERVER_ENABLED=.*/TURNSERVER_ENABLED=1/' /etc/default/coturn
systemctl enable --now coturn >/dev/null
systemctl restart coturn

if command -v ufw >/dev/null && ufw status | grep -q "Status: active"; then
  ufw allow 3478/udp >/dev/null && ufw allow 3478/tcp >/dev/null && ufw allow 49160:49260/udp >/dev/null
  echo "ufw: 3478/udp+tcp ve 49160-49260/udp açıldı."
fi

# p2-server.ini [turn]: sunucu dosyayı izler, yeniden başlatmadan uygular.
URLS="turn:$HOST:3478?transport=udp, turn:$HOST:3478?transport=tcp"
if [[ ! -f "$INI" ]]; then
  echo "p2-server.ini henüz yok; P2 Server bir kez başlatılıyor…"
  systemctl restart p2-server.service || true
  sleep 4
fi
if [[ -f "$INI" ]]; then
  cp "$INI" "$INI.turn-yedek"
  if grep -q '^\[turn\]' "$INI"; then
    sed -i -E "/^\[turn\]/,/^\[/{s#^urls[[:space:]]*=.*#urls = $URLS#;s#^secret[[:space:]]*=.*#secret = $SECRET#}" "$INI"
  else
    printf '\n[turn]\nurls = %s\nsecret = %s\nttl = 86400\n' "$URLS" "$SECRET" >>"$INI"
  fi
  echo "p2-server.ini [turn] güncellendi."
else
  echo "UYARI: $INI bulunamadı. [turn] bölümüne şunları yaz:" >&2
  echo "  urls = $URLS" >&2
  echo "  secret = $SECRET" >&2
fi
echo
echo "TURN hazır: turn:$HOST:3478 (coturn: systemctl status coturn)"
echo "Modemde 3478 UDP+TCP ve 49160-49260 UDP portlarını bu sunucuya yönlendir."
