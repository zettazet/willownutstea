# Audio, profiles and message input

This is an extension of the existing Win32/CPAL/Opus client and SQLite/TLS
server. Navigation, call controls, tray behavior, group calls and Alt+V remain.
The Audio / Profile button expands a compact panel inside the existing window;
it starts collapsed. No new audio engine, playback stream per person, service,
UI framework or image-hosting service is used.

## Audio

- Mic and Output sliders: 0–200%, initially 100%. Application gain only;
  Windows system volume is unchanged. Gain applies immediately, with a gentle
  peak limiter to keep boosted samples below full scale. Loud signals may be
  compressed at high gain.
- During a call the panel adds a playback slider for each remote participant,
  never yourself. Preferences use stable user IDs scoped to the server address.
  These gains apply before the shared mixer; other participants are unaffected.
- The input/output dropdowns include Default Windows Device. They are enumerated
  when the panel opens, not continuously. Close/reopen the panel to refresh.
  A specific choice is saved using the device name supported by CPAL 0.16.
  Devices with identical names cannot be distinguished by this backend.
- Changing devices drops both existing streams before opening one input and one
  output stream. Mute/deafen is applied before playback/capture starts. Switching
  can produce a short gap. Missing selections fall back to the current Windows
  default; no available hardware produces a visible error. An unplug error ends
  the call safely; reconnect the device and call again.
- Mixer callbacks reuse existing buffers; gains add no per-frame allocations or
  threads. Device switching is optional and does not change Windows defaults.

## Profiles and memory

Upload avatar / Remove avatar requires sign-in. PNG, JPEG and WebP are accepted
up to 5 MiB. The server checks the actual image format, decodability and bounds:
maximum width/height 4096, maximum 4 megapixels, decoder allocation limit 32 MiB.
Large/invalid images are rejected without replacing your previous picture.

Images are normalized to at most 256×256 and stored as JPEG (quality 80, at most
128 KiB). Transparency is flattened against the existing dark background. Only
this optimized image is stored in SQLite; the original is discarded. Replacing
an avatar replaces its single database row. Existing SQLite backups include it.
During calls, uploads send at most 1 KiB every 75 ms so voice traffic can run
between chunks; a large picture may take several minutes. Outside calls, bulk
chunks are used. The existing runtime timer runs only while an upload is active.
Uploads use bounded chunks on the existing authenticated TLS connection, with
a separate rate budget. No extra public port or external storage is needed.
Image processing uses the existing blocking pool outside the shared server lock;
a semaphore allows one normalization at a time without queuing full images or
blocking the connection’s signaling/voice loop. Busy uploads can be retried. Partial uploads are bounded to
two 5 MiB buffers, expire after 60 seconds of inactivity, and are discarded on disconnect.

Avatars appear beside participant names in the existing call area. Friends and
conversation lists retain their original native text controls. Initials are the
default avatar. Alt+V's label is still server-authoritative and appears with
avatars both enabled and disabled.

Show Avatars defaults ON and is saved locally. Turning it OFF immediately clears
the image cache and uses the original text-only participant display. It does not
delete your profile picture or affect other clients. No new images are requested,
decoded or rendered while OFF; already-in-flight replies are ignored without
image decoding. Only avatar revision metadata continues to synchronize.

Only current-call profiles are requested, never the whole user directory. The
cache has a hard ceiling of twenty 32×32 BGRA thumbnails (80 KiB pixel storage;
current calls use at most eight). It evicts profiles outside the call and reuses
unchanged revisions. There are no persistent GDI bitmaps per avatar. Temporary
encoded responses/decoder buffers and map overhead are additional to that bound.

## Messages

Enter and Send use the same existing send handler. Shift+Enter inserts a newline.
IME composition/confirmation and key-repeat Enter do not submit. Empty or
whitespace-only messages and messages over 4096 UTF-8 bytes remain rejected.
Accepted submissions clear/focus the input; a full local queue preserves it.
Rejected messages restore into an empty input; if another draft is present, the
rejected text stays in history marked “not sent — copy to retry”. This tracking
survives switching conversations, within the existing 32-message bound. Pending
text remains memory-only, as before; closing/logging out loses unsent drafts.

## Updating your existing Oracle server

New profile operations require the updated server. Update the server first; old
clients can still use ordinary calls/messages with it. Upgraded clients can use
ordinary calls/messages with the older server, but profile actions will fail.

In the existing repository checkout **on the Ubuntu VM**:

```bash
git pull --ff-only origin main
sudo bash mini-voice/deploy/oracle/install.sh \
  --domain YOUR_EXISTING_DOMAIN --email YOUR_EXISTING_EMAIL
sudo systemctl is-active mini-voice
```

Use exactly the same domain/email as the original installation. The installer
selects x64/ARM64, checks hashes and retains the existing config/database. The
restart ends active calls. SQLite adds the avatars table automatically; no
account reset or manual migration is needed. Make a backup before updating as
described in HOSTING.md. Do not recreate the VM or replace your TLS certificate
manually. Download the updated client or portable ZIP after the server update.

## Verification limits

See VALIDATION.md for tests and executed checks. Native Windows devices, real
hardware hotplug, real IME software, residential network latency and Windows
RAM/CPU differences require testing on Windows; numeric improvements are not
claimed from a Linux/Wine build environment.
