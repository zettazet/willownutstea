#!/usr/bin/env bash
# Add only Mini Voice inbound rules; preserve SSH and existing firewall policy.
set -euo pipefail
case ${1:-} in
  --dry-run)
    [[ $# == 1 ]] || exit 1
    echo 'Plan: allow TCP 80, TCP 4433 and UDP 4434 in active UFW or INPUT iptables; persist rules. No changes made.'
    exit 0;;
  --apply) [[ $# == 1 ]] || exit 1;;
  *) echo 'Usage: sudo bash open-firewall.sh --apply | bash open-firewall.sh --dry-run' >&2; exit 1;;
esac
((EUID == 0)) || { echo 'Run --apply with sudo' >&2; exit 1; }
# shellcheck source=/dev/null
source /etc/os-release
[[ ${ID:-} == ubuntu && ${VERSION_ID:-} == 24.04 ]] || { echo 'Requires Ubuntu 24.04' >&2; exit 1; }
if command -v ufw >/dev/null && LC_ALL=C ufw status | grep '^Status: active' >/dev/null; then
  ufw allow 80/tcp
  ufw allow 4433/tcp
  ufw allow 4434/udp
else
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends iptables-persistent
  for port in 80 4433; do
    iptables -C INPUT -p tcp --dport "$port" -j ACCEPT 2>/dev/null || iptables -I INPUT 1 -p tcp --dport "$port" -j ACCEPT
  done
  iptables -C INPUT -p udp --dport 4434 -j ACCEPT 2>/dev/null || iptables -I INPUT 1 -p udp --dport 4434 -j ACCEPT
  netfilter-persistent save
fi
echo 'VM firewall rules added. OCI NSG/security-list ingress must also be configured in the Oracle console.'
