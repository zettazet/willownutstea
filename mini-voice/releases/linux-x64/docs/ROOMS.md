# goyvoice: persistent rooms and this update

**Update your server first, then every Windows client.** This version adds a
SQLite `voice_rooms` table automatically without changing existing accounts,
friends, chats or avatars. The installation/service/config paths remain
`mini-voice` for compatibility. Back up your database before updating.

Follow [INSTALL-UPDATE.md, A](INSTALL-UPDATE.md#a-update-your-existing-oracle-server)
for a managed Oracle install. Use its binary-only update section for a custom
server config. Use the new Linux binary for your VM architecture; the Windows
ZIP's `mini-server.exe` is not for Oracle. Existing `server.json` files continue
to work: the new room settings have defaults. Restarting interrupts active calls.

Then use [INSTALL-UPDATE.md, C](INSTALL-UPDATE.md#c-install-or-update-the-windows-client).
Choose **Exit app** before replacing files, then launch **goyvoice.exe**. The
old `mini-client.exe` name remains an identical compatibility copy; run only one.
Settings/tokens still use `%LOCALAPPDATA%\MiniVoice`, so your login and audio
preferences survive the rename. The UI, tray, dialogs and Windows file metadata
now say goyvoice. No new app renderer or audio threads were added.

## Create and use a room

1. Sign in as **judas**, the configured creator account.
2. Click **Chats / show rooms** in the left sidebar.
3. Enter a room name (1–48 characters) and creation password **231939**.
4. Click **Create room**. The password field clears and is not saved locally.
   Creating a room does not automatically join it or turn on your microphone.
5. Select the room and click **Join room**. Your accepted friends see the room
   on the same server and join it the same way; no invitation or creation PIN
   is needed for joining.
6. **Leave** leaves the room without deleting it. The room remains available
   while empty, while you are offline, and across server restarts. Your PC can
   be off; the cloud server must still be running and reachable.
7. Only the owner can **Delete room**. Confirm the dialog: deletion removes the
   room permanently and ends participation for everyone inside it.

Room entries show **`[2/8 LIVE] Lounge — judas`** or **`[0/8 empty] Lounge — judas`**.
Counts update from server snapshots as people join, leave, log out or disconnect.
The count is at the beginning so long room names do not hide it. An abruptly
lost connection may remain counted until the server detects its timeout.

Rooms use the existing call audio, mute/Insert, deafen, noise cleaner, avatars,
per-person volume, encrypted UDP and TLS fallback. One person can be in only one
call or room at a time; leave before switching. Each room holds at most eight
people. The server permits at most 32 persistent rooms, with a total limit of
128 room/call records. Empty rooms contain metadata, not active audio decoders
or worker threads. There is no new room text-chat/history channel; existing
conversations remain separate and keep their history.

Rooms are visible/joinable only to their owner and the owner's **accepted
friends**. Friends of participants do not gain access indirectly. Password
possession alone does not grant creation rights: the server checks the signed-in
account against `judas` first, then checks the PIN. Friends cannot delete rooms.
The creation PIN does not replace your account password or encrypt the call.
Protect the judas account with a strong account password.

## Optional server configuration

The defaults already match this installation: creator `judas`, PIN `231939`.
An operator can add `room_creator_username` and `room_creation_password_hash`
(SHA-256 hex, not a plaintext PIN) to `server.json`. To calculate a replacement:

```bash
python3 - <<'PY'
import getpass, hashlib
print(hashlib.sha256(getpass.getpass('New room creation password: ').encode()).hexdigest())
PY
```

Edit `/etc/mini-voice/server.json` with `sudoedit`, preserving its other fields and
permissions. Validate with `sudo -u mini-voice /usr/local/bin/mini-server
--check-config /etc/mini-voice/server.json`, then `sudo systemctl restart mini-voice`.
A custom config needs the guide's binary-only update route because the installer
refuses to overwrite custom configuration. Changing the configured creator does
not transfer ownership of existing rooms.

## Message sound and participant names

Live incoming messages play a short, soft message tone even if another chat is
selected or the window is in the tray. Your own messages and loaded history do
not trigger it; repeated message IDs are suppressed with a bounded 64-ID cache,
and rapid arrivals are coalesced to at most one tone per 300ms. Audio / Profile
→ **Soft sounds** controls it along with existing call/mic cues. Deafen suppresses
new notification sounds. Sounds play through the default Windows playback
session, not the call's Output slider.

Names in ordinary calls and rooms are **bold**, with eight different colors for
the eight participant positions. This also works with Show Avatars OFF; names
and mute labels remain readable text. One shared bold font is reused, with no
per-person bitmap cache or rendering loop.
