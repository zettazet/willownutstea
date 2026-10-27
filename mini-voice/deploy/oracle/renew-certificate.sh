#!/usr/bin/env bash
# Certbot calls this as root only after successfully issuing/renewing a certificate.
set -euo pipefail
configured_domain=$(cat /etc/mini-voice/domain)
lineage="/etc/letsencrypt/live/$configured_domain"
[[ ${RENEWED_LINEAGE:-} == "$lineage" ]] || exit 0
install -o root -g mini-voice -m 0640 "$lineage/fullchain.pem" /etc/mini-voice/tls/fullchain.pem.next
install -o root -g mini-voice -m 0640 "$lineage/privkey.pem" /etc/mini-voice/tls/privkey.pem.next
mv -f /etc/mini-voice/tls/fullchain.pem.next /etc/mini-voice/tls/fullchain.pem
mv -f /etc/mini-voice/tls/privkey.pem.next /etc/mini-voice/tls/privkey.pem
runuser -u mini-voice -- /usr/local/bin/mini-server --check-config /etc/mini-voice/server.json
if [[ ${MINIVOICE_DEFER_RESTART:-0} != 1 ]] && systemctl is-active --quiet mini-voice.service; then
  systemctl restart mini-voice.service
fi
