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
cache has a hard ceiling of twenty 40×40 BGRA thumbnails (125 KiB pixel storage;
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

## Voice cleanup and square avatars

The cocoa/peach UI uses native Win32 controls, one shared Segoe UI font and
GDI-painted rounded buttons. There is no web renderer, animation timer or image
background. Section headings separate friends, conversations and the message area.

**Insert** toggles microphone mute during a call, including while hidden in the
tray or while another app has focus. Holding the key does not repeatedly toggle.
The shortcut is global while this client runs. If another application owns Insert,
a status message explains the conflict; the mic button and an in-window Insert
fallback remain available. Outside calls the key does nothing in Mini Voice.

**Soft sounds** (Audio / Profile, ON by default) plays the same quiet 220ms chime
when you join/leave a call, a participant joins/leaves your current call, or the
connection to your active call drops. Mute has a descending tone; unmute has an
ascending tone. No ringing or repeated alarm. New sounds replace the previous
sound instead of stacking. Sounds are local, not transmitted to the call, and
use the default Windows playback device/session volume, independently of the
selected call output and Output slider. Headphones prevent acoustic mic pickup.
Deafen suppresses new sounds. Disabling Soft sounds stops the current sound.
The three original PCM cues total about 21 KiB embedded; no application sound
thread or sound queue is created.

**Voice cleanup** defaults ON and affects only your outgoing microphone before
the mic gain, limiter and Opus encoder. It combines 80Hz high-pass / 8kHz low-pass
filtering, smooth downward expansion and an **RNNoise cleaner** (also ON by
default). RNNoise uses the embedded `nnnoiseless` model; no download, network
service or extra audio thread. It processes 10ms mono 48kHz blocks within the
existing 20ms encoder frame, with approximately 10ms algorithmic delay. State is
created for an active call and dropped with its audio streams. Steady processing
reuses its buffers and FFT cache; mute/bypass transitions recreate the small
recurrent state to clear captured audio. The FFT library keeps a small per-thread
cache until that audio thread exits. RNNoise costs additional CPU; turn off just
RNNoise cleaner to keep the basic filters, or turn off Voice cleanup to bypass
both. Changes apply live and settings persist.

**Threshold** ranges from **−60 to −20 dBFS**, default **−50**. Start at −50, speak
normally, then move toward −40/−30 if room noise still leaks between words. Moving
toward −60 preserves softer speech. Excessively high thresholds can suppress
quiet syllables. This is a smooth expander, not a hard voice-activation switch;
it lowers quiet input rather than stopping network packets. RNNoise reduces
background noise but cannot guarantee removing every keystroke or competing
voice. Neither mode is echo cancellation; use headphones.

Each person must update their own client to clean their microphone. Your local
playback volume controls continue to work independently. No server update needed
for this theme, shortcut, sounds or microphone update.

Call avatars are now 40×40 squares (25% larger per side), including the initials
placeholder. The call area gains 16 pixels for two rows of four participants;
Show Avatars OFF uses the original text spacing and still frees cached images.
This update is entirely client-side; the server does not need another update.
