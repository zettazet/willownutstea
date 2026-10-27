#!/usr/bin/env bash
# Ubuntu 24.04 OCI VM; no cloud API, paid resources, or firewall mutation.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd -- "$script_dir/../.." && pwd)
domain='' email='' binary='' arch='' dry_run=false
fail() { echo "Error: $*" >&2; exit 1; }
usage() {
  cat <<'HELP'
Usage: sudo bash install.sh --domain voice.example.org --email you@example.org
       bash install.sh --domain voice.example.org --email you@example.org --dry-run
Options: --binary PATH (custom trusted build), --arch x64|arm64 (plan architecture),
         --dry-run (validate arguments/checksum without changes), --help
Requires Ubuntu 24.04, correct DNS A record, inbound TCP 80/4433 and UDP 4434
in OCI NSG/security lists AND the VM's firewall. Does not create an Oracle account.
HELP
}
while (($#)); do
  case "$1" in
    --domain|--email|--binary|--arch)
      if (($# < 2)); then fail "Missing value for $1"; fi
      if [[ -z $2 || $2 == --* ]]; then fail "Missing value for $1"; fi
      case "$1" in --domain) domain=$2;; --email) email=$2;; --binary) binary=$2;; --arch) arch=$2;; esac
      shift 2;;
    --dry-run) dry_run=true; shift;;
    --help|-h) usage; exit 0;;
    *) fail "Unknown argument: $1";;
  esac
done
[[ -n $domain && -n $email ]] || { usage >&2; exit 1; }
[[ ${#domain} -le 253 && $domain == *.* && $domain != .* && $domain != *. && ! $domain =~ ^[0-9.]+$ ]] || fail 'Use a DNS hostname, not an IP or localhost'
IFS='.' read -r -a labels <<< "$domain"
for label in "${labels[@]}"; do
  [[ ${#label} -le 63 && $label =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$ ]] || fail 'Domain requires lowercase DNS labels without leading/trailing hyphens'
done
[[ $email =~ ^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$ ]] || fail 'Enter a valid ACME email'
case $(uname -m) in x86_64) host_arch=x64;; aarch64|arm64) host_arch=arm64;; *) fail 'Supported architectures: x86_64 and aarch64';; esac
arch=${arch:-$host_arch}
[[ $arch == x64 || $arch == arm64 ]] || fail 'Architecture must be x64 or arm64'
if ! "$dry_run"; then
  [[ $arch == "$host_arch" ]] || fail 'Requested binary architecture does not match this VM'
fi
if [[ -z $binary ]]; then
  binary="$project_root/releases/linux-$arch/mini-server"
  [[ -f $binary ]] || fail "Missing bundled binary: $binary"
  (cd -- "$project_root/releases/linux-$arch" && sha256sum --check BINARY-SHA256SUMS.txt) || fail 'Bundled binary checksum mismatch'
fi
[[ -f $binary ]] || fail "Binary not found: $binary"
binary=$(realpath -- "$binary")
if "$dry_run"; then
  cat <<PLAN
Plan only; no changes made:
Architecture: $arch; binary: $binary
DNS: $domain; ACME email: $email
Install certbot/ca-certificates/openssl/sqlite3 through apt.
Run as unprivileged mini-voice user via mini-voice.service.
Database: /var/lib/mini-voice/mini-voice.sqlite (preserved across updates).
Config: /etc/mini-voice/server.json; TLS: /etc/mini-voice/tls/.
Enable certbot.timer and renewal hook; client: wss://$domain:4433.
Configure OCI ingress and VM firewall separately; this script does not modify either.
PLAN
  exit 0
fi
((EUID == 0)) || fail 'Run with sudo; --dry-run does not require root'
# shellcheck source=/dev/null
source /etc/os-release
[[ ${ID:-} == ubuntu && ${VERSION_ID:-} == 24.04 ]] || fail 'Installer requires Ubuntu 24.04 LTS'
if [[ -e /etc/mini-voice/server.json ]]; then
  [[ -f /etc/mini-voice/domain && $(cat /etc/mini-voice/domain) == "$domain" ]] || fail 'Existing unmanaged config or different domain: refusing to overwrite'
  cmp --silent /etc/mini-voice/server.json "$script_dir/server.example.json" || fail 'Existing custom config differs; update manually or keep a reviewed backup first'
fi
getent ahostsv4 "$domain" >/dev/null || fail 'No IPv4 address found for domain'
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends certbot ca-certificates openssl sqlite3
if ! id mini-voice >/dev/null 2>&1; then
  useradd --system --user-group --home-dir /var/lib/mini-voice --no-create-home --shell /usr/sbin/nologin mini-voice
fi
install -d -o root -g mini-voice -m 0750 /etc/mini-voice /etc/mini-voice/tls
install -d -o mini-voice -g mini-voice -m 0700 /var/lib/mini-voice
install -o root -g root -m 0755 "$binary" /usr/local/bin/mini-server.next
if [[ -e /usr/local/bin/mini-server ]]; then
  cp --preserve=mode,ownership /usr/local/bin/mini-server /usr/local/bin/mini-server.previous
fi
mv -f /usr/local/bin/mini-server.next /usr/local/bin/mini-server
if [[ ! -e /etc/mini-voice/server.json ]]; then
  install -o root -g mini-voice -m 0640 "$script_dir/server.example.json" /etc/mini-voice/server.json
fi
printf '%s\n' "$domain" > /etc/mini-voice/domain
chmod 0644 /etc/mini-voice/domain
install -o root -g root -m 0644 "$script_dir/mini-voice.service" /etc/systemd/system/mini-voice.service
install -d -o root -g root -m 0755 /etc/letsencrypt/renewal-hooks/deploy
install -o root -g root -m 0750 "$script_dir/renew-certificate.sh" /etc/letsencrypt/renewal-hooks/deploy/mini-voice.sh
systemctl daemon-reload
MINIVOICE_DEFER_RESTART=1 certbot certonly --standalone --preferred-challenges http --non-interactive --agree-tos \
  --email "$email" --cert-name "$domain" -d "$domain" --keep-until-expiring
# A certificate that is not yet due for renewal may not run deploy hooks.
MINIVOICE_DEFER_RESTART=1 RENEWED_LINEAGE="/etc/letsencrypt/live/$domain" /etc/letsencrypt/renewal-hooks/deploy/mini-voice.sh
systemctl enable --now certbot.timer
systemctl enable mini-voice.service
systemctl restart mini-voice.service
sleep 1
systemctl is-active --quiet mini-voice.service || fail 'Service failed; inspect journalctl -u mini-voice'
printf 'Installed. Client: wss://%s:4433\nLogs: sudo journalctl -u mini-voice -f\n' "$domain"
