# Complete installation and update guide

goyvoice has two parts: **one Linux server on your Oracle VM**, and a
**Windows 10/11 x64 client on each person's PC**. Friends install only the client.
The Windows ZIP also contains `mini-server.exe`; the Oracle VM uses the Linux
binary selected automatically by the installer.

**For persistent rooms, update the server first (A), then the Windows clients
(C–E).** Existing configs default to room creator `judas` and creation PIN
`231939`; no config edit is required. See [ROOMS.md](ROOMS.md) for creation,
live occupancy counts and deletion. A server from before avatar support also
needs section A. Follow B only for a fresh installation. Replace domain, email,
IP and SSH-key examples with your own values.

## A. Update your existing Oracle server

### A1. Connect from Windows

Open **PowerShell on your PC**:

```powershell
ssh -i "$env:USERPROFILE\Downloads\oracle.key" ubuntu@YOUR_VM_PUBLIC_IP
```

Use the same SSH key and IP you used originally. The following bash commands run
**inside the Ubuntu SSH session**. Updating restarts the service and ends current
calls; arrange a quiet time. Accounts, friends, history and avatars are stored in
the existing database, outside the Git checkout.

Check the installation and recover the existing domain:

```bash
sudo systemctl status mini-voice --no-pager
cat /etc/mini-voice/domain
cat /etc/os-release
uname -m
```

The installer supports Ubuntu **24.04 LTS**, with `x86_64` or `aarch64`/`arm64`.
Write down the domain shown by the second command and use the email associated
with your existing certificate. Keep the same server address in the clients.

### A2. Back up before updating

These are the managed installer’s default paths. For a custom configuration,
use its actual database path. The commands below create a consistent backup
while the server runs:

```bash
backup_stamp=$(date -u +%Y%m%dT%H%M%SZ)
sudo install -d -m 0700 /var/backups/mini-voice
sudo sqlite3 -readonly /var/lib/mini-voice/mini-voice.sqlite \
  ".backup '/var/backups/mini-voice/backup-$backup_stamp.sqlite'"
sudo sqlite3 -readonly "/var/backups/mini-voice/backup-$backup_stamp.sqlite" \
  'PRAGMA integrity_check;'
echo "/var/backups/mini-voice/backup-$backup_stamp.sqlite"
```

Expect `ok`. Record the printed filename. Do not copy only the live `.sqlite`
file while writes are in progress: recent data may be in its WAL file.

Also preserve the service/configuration and certificates:

```bash
sudo tar -czf "/var/backups/mini-voice/config-$backup_stamp.tar.gz" \
  -C / etc/mini-voice etc/letsencrypt etc/systemd/system/mini-voice.service
sudo chmod 0600 "/var/backups/mini-voice/config-$backup_stamp.tar.gz"
```

The configuration archive contains private keys; keep it private. A backup on the
same VM does not protect against VM deletion. To download the database backup,
first export a protected temporary copy **on Ubuntu**:

```bash
sudo install -o ubuntu -g ubuntu -m 0600 \
  "/var/backups/mini-voice/backup-$backup_stamp.sqlite" \
  /home/ubuntu/mini-voice-backup.sqlite
```

In a **second PowerShell window on your PC**:

```powershell
$backupDir = "$env:USERPROFILE\Downloads\MiniVoice-backups"
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
scp -i "$env:USERPROFILE\Downloads\oracle.key" ubuntu@YOUR_VM_PUBLIC_IP:/home/ubuntu/mini-voice-backup.sqlite "$backupDir\mini-voice-backup.sqlite"
```

Keep dated copies on your PC. After confirming the download, remove only the
exported temporary copy on Ubuntu:

```bash
rm -- /home/ubuntu/mini-voice-backup.sqlite
```

You can export/download the configuration archive using the same `install -m
0600` and SCP pattern; never put it in GitHub, a public folder or chat.

### A3. Get the update

In your **Ubuntu SSH session**, enter the original repository checkout:

```bash
cd ~/willownutstea
git status --short --branch
git switch main
git pull --ff-only origin main
git log -1 --oneline
```

If you cloned somewhere else, use that directory. If Git reports local edits or
cannot fast-forward, preserve those changes and resolve the reported conflict
before continuing. Do not use `git reset --hard` as a routine update step.
Running the installer alone does not fetch new files.

### A4. Install the updated Linux server

Replace the example domain/email with the existing ones. Optional preview:

```bash
bash mini-voice/deploy/oracle/install.sh \
  --domain voice.your-domain.net --email you@example.net --dry-run
```

Apply the update:

```bash
sudo bash mini-voice/deploy/oracle/install.sh \
  --domain voice.your-domain.net --email you@example.net
```

Continue only if the installer completes successfully and prints `Installed. Client: ...`. An already-active older process alone does not prove the update
succeeded.

The installer checks the binary hash, chooses x64/ARM64, preserves the standard
configuration and database, and restarts `mini-voice`. The database gains its
avatars table automatically. You keep existing usernames/passwords and history.
Firewall rules do not need changing for avatars; all profile traffic uses the
existing TLS port 4433. Keep TCP 80 available for certificate renewal.

If the installer says **custom/unmanaged configuration** or **different domain**,
stop this installation path. Do not replace your configuration just to bypass the
check. Verify the domain; use section F for a binary-only update of a reviewed
custom installation.

### A5. Verify the update

```bash
sudo systemctl is-enabled mini-voice
sudo systemctl is-active mini-voice
sudo systemctl status mini-voice --no-pager
sudo -u mini-voice /usr/local/bin/mini-server \
  --check-config /etc/mini-voice/server.json
sudo journalctl -u mini-voice -n 60 --no-pager
```

Expect `enabled`, `active`, and a valid configuration/TLS key pair. Config checks
alone do not establish external reachability; complete the client checks in E.

## B. Install a fresh Oracle server

Use an existing account or verify Oracle's current registration/free-tier terms.
Oracle may require a card; the application cannot bypass that. Choose only
resources your console explicitly identifies as covered by your free allowance,
including compute, storage and networking. Do not rely on trial credits to
promise a permanently free server.

1. Create a Compute VM using **Ubuntu 24.04 LTS** and an eligible x64 or ARM64
   shape. Save its SSH private key and public IPv4. The subnet needs a public IP,
   an Internet Gateway and an outbound route through that gateway.
2. Create a DNS **A** record, such as `voice.your-domain.net`, pointing at that
   public IPv4. An existing domain/subdomain or supported free DDNS hostname is
   sufficient. Use **DNS only**, without a CDN proxy. Configure IPv6/AAAA only if
   you also configure the server to accept IPv6.
3. In the VM's attached Oracle NSG or subnet Security List, add stateful ingress:

| Protocol | Destination port | Source | Purpose |
|---|---:|---|---|
| TCP | 22 | Your public IP/32 | SSH |
| TCP | 80 | `0.0.0.0/0` | Certificate issuance/renewal |
| TCP | 4433 | `0.0.0.0/0` | Login, messages, profiles, TLS voice fallback |
| UDP | 4434 | `0.0.0.0/0` | Voice |

Leave source ports as **All**. Preserve SSH access and normal outbound access.
Oracle network rules and Ubuntu firewall rules are separate layers.

Connect from PowerShell as in A1. Then run **on Ubuntu**:

```bash
sudo apt-get update
sudo apt-get install -y git ca-certificates
git clone --branch main --single-branch https://github.com/zettazet/willownutstea.git
cd willownutstea
getent ahostsv4 voice.your-domain.net
```

Replace the hostname first. The DNS result must be your VM's public IPv4. Then:

```bash
sudo bash mini-voice/deploy/oracle/open-firewall.sh --apply
sudo bash mini-voice/deploy/oracle/install.sh \
  --domain voice.your-domain.net --email you@example.net
```

TCP 80 must be reachable and free for Certbot's standalone HTTP challenge. If an
existing web server occupies it, configure ACME for that installation instead of
stopping unrelated services blindly. Verify the service with A5, then continue
with C. See [HOSTING.md](HOSTING.md) for detailed Oracle networking instructions.

## C. Install or update each Windows client

Use Windows **10/11 x64**. Rust, Node.js, Python and an Oracle account are not
needed on friends' PCs.

- [Download goyvoice.exe](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/windows-x64/goyvoice.exe)
- [Download the complete Windows ZIP](https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/mini-voice-windows-x64.zip)

For an update:

1. Finish the server update first so profile operations are supported.
2. Leave calls and copy any unsent draft text somewhere safe. Pending drafts are
   memory-only.
3. In the old client, click **Exit app**, or right-click its tray icon near the
   clock and choose **Exit (end calls)**. The window's X only hides it.
4. Extract the **entire new ZIP** into a folder you control, such as
   `%USERPROFILE%\Apps\MiniVoice`. Replace the old package only after it exits.
   Keep its license files/notices when redistributing it. Do not run inside the
   ZIP preview. If using the standalone EXE, replace the old EXE after exiting.
5. Run `goyvoice.exe`. The Oracle VM does not use the ZIP's Windows server.

Existing local settings are in:

```text
%LOCALAPPDATA%\MiniVoice\settings.json
```

Replacing the application folder preserves these settings. Do not delete that
file to update. It contains a Windows-protected session token and your local
preferences; keep it private. Copying it to another Windows account/PC does not
guarantee that account can decrypt the token. Sign in normally on a new PC.

If you prefer PowerShell downloading, **after exiting the old app**:

```powershell
$ErrorActionPreference = 'Stop'
$clientDir = "$env:USERPROFILE\Apps\MiniVoice"
$zipPath = "$env:TEMP\mini-voice-windows-x64.zip"
New-Item -ItemType Directory -Force -Path $clientDir | Out-Null
Invoke-WebRequest -UseBasicParsing -Uri "https://raw.githubusercontent.com/zettazet/willownutstea/main/mini-voice/releases/mini-voice-windows-x64.zip" -OutFile $zipPath
Expand-Archive -LiteralPath $zipPath -DestinationPath $clientDir -Force
Start-Process -FilePath "$clientDir\goyvoice.exe"
```

You can verify `goyvoice.exe` and `mini-server.exe` with `Get-FileHash
-Algorithm SHA256` against the package's `BINARY-SHA256SUMS.txt`. The ZIP's hash is
published alongside it as `mini-voice-windows-x64.zip.sha256` in the repository.
These are unsigned builds; review any Windows security prompt before running.

## D. Sign in and use the new controls

Use the same exact address on every PC:

```text
wss://voice.your-domain.net:4433
```

An existing saved session may sign in automatically. Otherwise, for an existing
account, use **Log in**. For a new account, use **Register**:
username 3–24 ASCII letters/digits/underscore, password 10–128 UTF-8 bytes.
Each person should have their own account; signing in elsewhere replaces that
account's existing connection.

**Friends and chats:** Add friend → the other person chooses Accept friend.
Select friends with Ctrl-click → New chat. Open the conversation. Enter or Send
submits; Shift+Enter adds a newline. Empty/whitespace-only messages are rejected.

**Calls:** Call → invitees choose Accept call. Invite to call adds selected
friends during a call, up to eight total participants. Mute, Deafen and Leave
retain their existing meanings. Use headphones; echo cancellation is not built in.

**Audio / Profile:** expand the compact panel. Mic and Output default to 100%
and support 0–200%, independently of Windows system volume. Remote participant
sliders appear during calls and are saved by server/user ID. Device selectors
include Default Windows Device. Changing devices may produce a brief gap; close
and reopen the panel to refresh the device list after plugging in hardware.

**Voice cleanup:** defaults ON, with an RNNoise cleaner for background noise.
Audio / Profile → Threshold defaults to −50 dBFS; move toward −30 to suppress
more background noise, or toward −60 if quiet speech is cut. Changes apply live.
Turn off RNNoise cleaner to reduce CPU while keeping basic filters; Voice cleanup
OFF bypasses both. These settings are saved. Use headphones; this is not echo
cancellation. Everyone updates their own client to clean their own microphone.

**Insert:** mute/unmute during a call, also from the tray or another application.
If another app has reserved Insert, the status line reports it; use the mic button.
Holding Insert does not repeatedly toggle. Soft sounds in Audio / Profile controls
the short call join/leave/participant chime and distinct mute/unmute tones. These
play on the default Windows output, with Windows session volume (not the call
Output slider). They are suppressed while deafened. Sounds default ON and persist.

**Avatars:** sign in, then Upload avatar. Use PNG/JPEG/WebP up to 5 MiB, at most
4096 pixels on either side and at most 4 megapixels total. The server stores an
optimized image of at most 256×256. Avatars show as 40-pixel squares in calls; friends/conversation
lists remain text. Remove avatar deletes your stored picture. Show Avatars only
affects your own display and frees the image cache when OFF; it does not delete
your picture. Large uploads take longer during calls because voice is prioritized.

**Background mode:** X, minimize and Alt+F4 hide the client to the notification
area. Calls and microphone continue in their previous state. Click the icon to
reopen it; use Exit app to stop it fully. This is not Windows autostart. The
client must remain running, and a sleeping/offline PC cannot participate in calls.

## E. Confirm everything works

For the latest client, also verify Insert toggles mute once per press while the
window is open and while in the tray. Listen for the soft call chime when joining,
leaving and when a friend joins, and descending/ascending mic tones. Disable Soft
sounds and confirm silence. In a quiet call, try RNNoise ON/OFF and adjust the
threshold while speaking softly; lower it if syllables disappear. Restart the
client to confirm your choices persist. Check the expanded panel at the smallest
window size and with your Windows display scaling.

Use two different PCs/accounts, preferably on different internet connections:

1. Log in and check existing friends/history.
2. Send with Enter, then send a multiline message using Shift+Enter and Send.
3. Start a call. Test microphone mute and deafen separately.
4. Adjust Mic/Output, then each remote participant's playback volume.
5. Change devices, verify the selected hardware, and return to Windows defaults.
6. Upload an avatar; check the other client sees it in the call. Turn Show Avatars
   OFF on one client; its display changes while the other client keeps its own
   setting. Turn it back ON, then test removing the uploaded avatar.
7. Exit/reopen the client; confirm preferences and normal login/history persist.
8. Test a group call and tray hide/restore. Confirm microphone state before
   leaving the app in the background.

After a server restart, create a new call. Clients can reconnect, but the restart
ends transient calls. Closing SSH or turning off your own PC does not stop the
Oracle VM's system service.

Useful Ubuntu commands:

```bash
sudo systemctl restart mini-voice
sudo systemctl stop mini-voice
sudo systemctl start mini-voice
sudo journalctl -u mini-voice -f
sudo systemctl list-timers certbot.timer
sudo certbot renew --dry-run
```

Ctrl+C leaves live log viewing. Keep DNS pointing at the VM and TCP 80 open for
automatic certificate renewal. Successful renewal can restart the service and
end calls. Oracle uptime/resource retention depends on its policies; keep
backups outside the VM.

## F. Custom configuration: binary-only update

Use this only when your reviewed installation already has the service and
correct custom config/TLS paths, and the installer refuses to overwrite them.
First back up as in A2 and pull the update in the checkout as in A3.

```bash
case "$(uname -m)" in
  x86_64) server_arch=x64 ;;
  aarch64|arm64) server_arch=arm64 ;;
  *) echo 'Unsupported architecture'; exit 1 ;;
esac
(cd "mini-voice/releases/linux-$server_arch" && sha256sum --check BINARY-SHA256SUMS.txt)
```

Proceed only if checksum verification succeeds. Prepare and validate:

```bash
sudo install -o root -g root -m 0755 \
  "mini-voice/releases/linux-$server_arch/mini-server" /usr/local/bin/mini-server.next
sudo -u mini-voice /usr/local/bin/mini-server.next \
  --check-config /etc/mini-voice/server.json
```

Proceed only after validation succeeds. The paths below assume your existing
unit runs `/usr/local/bin/mini-server`; inspect `sudo systemctl cat mini-voice`
and use the actual paths if yours differ.

```bash
sudo systemctl stop mini-voice
sudo cp --preserve=mode,ownership /usr/local/bin/mini-server /usr/local/bin/mini-server.previous
sudo mv -f /usr/local/bin/mini-server.next /usr/local/bin/mini-server
sudo systemctl start mini-voice
sudo systemctl status mini-voice --no-pager
```

This replaces the executable while retaining custom config, service and TLS
setup. It does not install missing dependencies, firewall rules or renewal hooks.

## G. Rollback and troubleshooting

### Roll back only the executable

The installer/binary-update procedure saves the immediately preceding binary as
`/usr/local/bin/mini-server.previous`. If present, restore it with:

```bash
sudo test -f /usr/local/bin/mini-server.previous
```

Continue only if the file exists:

```bash
sudo systemctl stop mini-voice
sudo cp --preserve=mode,ownership /usr/local/bin/mini-server.previous /usr/local/bin/mini-server.rollback
sudo mv -f /usr/local/bin/mini-server.rollback /usr/local/bin/mini-server
sudo systemctl start mini-voice
sudo systemctl status mini-voice --no-pager
```

For this avatar update, the schema addition is additive; the previous server can
ignore the extra table. Do not restore an older database merely to roll back the
binary: doing so loses messages/accounts created since the backup. Avatar
operations need the updated server again.

### Restore a database only when necessary

Set the exact backup path and verify it first:

```bash
backup_file=/var/backups/mini-voice/YOUR_BACKUP.sqlite
sudo test -s "$backup_file"
sudo sqlite3 -readonly "$backup_file" 'PRAGMA integrity_check; SELECT count(*) FROM users;'
```

Proceed only when the intended existing backup passes both commands, reports
`ok`, and shows the expected account count. Database recovery
replaces current data with the state at backup time. Preserve the existing
SQLite/WAL/SHM together before replacing it:

```bash
sudo systemctl stop mini-voice
recovery_dir="/var/backups/mini-voice/pre-restore-$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -m 0700 "$recovery_dir"
for data_file in /var/lib/mini-voice/mini-voice.sqlite /var/lib/mini-voice/mini-voice.sqlite-wal /var/lib/mini-voice/mini-voice.sqlite-shm; do
  if sudo test -e "$data_file"; then sudo mv -- "$data_file" "$recovery_dir/"; fi
done
sudo install -o mini-voice -g mini-voice -m 0600 "$backup_file" /var/lib/mini-voice/mini-voice.sqlite
sudo systemctl start mini-voice
sudo systemctl status mini-voice --no-pager
```

### Common problems

| Problem | Check |
|---|---|
| New controls are missing | Exit every old instance; run the newly extracted EXE and open Audio / Profile. |
| Avatar action says malformed request | Update/restart the server as well as the client. |
| Avatar rejected | File size, actual PNG/JPEG/WebP data, 4096-side/4-megapixel bounds; try a smaller image outside a call. |
| Avatar not visible | Join a call, enable Show Avatars, wait for upload/download; lists remain text. |
| Cannot replace EXE | Exit app/tray Exit; X only hides the running process. |
| Client cannot connect | Same `wss://hostname:4433`, correct DNS A, DNS-only proxy mode, TCP 4433 through both firewalls, matching valid certificate. |
| Voice fails | UDP 4434 through both firewalls, correct devices, mute/deafen state, Windows microphone permissions for desktop apps. |
| Device disappears | Reconnect it, reopen Audio / Profile, select available hardware and call again. |
| TLS renewal fails | Correct DNS and reachable/unoccupied TCP 80; inspect Certbot logs and timer. |
| Service will not start | `sudo journalctl -u mini-voice -n 100 --no-pager`; check config/TLS, permissions and architecture. |
| Client crashes | Open `%LOCALAPPDATA%\MiniVoice\client.log` using Win+R; native loader failures may require Windows Event Viewer. |

Native Windows hardware/IME, Oracle provisioning/ACME and Windows RAM/CPU have
not been fully exercised by the build environment. Executed automated checks
and the Wine limitation for DPAPI are recorded in [VALIDATION.md](VALIDATION.md).
