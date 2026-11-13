#!/usr/bin/env bash
set -euo pipefail
if [[ ${EUID:-$(id -u)} -ne 0 ]]; then echo "sudo bash uninstall.sh" >&2; exit 1; fi
systemctl disable --now p2-server.service 2>/dev/null || true
rm -f /etc/systemd/system/p2-server.service /usr/local/sbin/p2-serverctl
rm -f /opt/p2-server/p2-server
systemctl daemon-reload
echo "Uygulama kaldırıldı. Veriler güvenlik için /var/lib/p2-server ve /etc/p2-server altında bırakıldı."
