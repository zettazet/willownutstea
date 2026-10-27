# Validation and remaining acceptance work

Linux build environment: Debian 13 x64, Rust 1.99.1, GCC, CMake 4.4.4.
Windows cross target: x86_64-pc-windows-gnu, GCC MinGW-w64 14, release profile.
A native Windows runtime, physical audio devices and separate residential
networks are unavailable here. A Wine smoke test is described below. An executable being built is not proof it runs correctly
on either supported Windows version.

## Automated checks

Run from `mini-voice` with a C/C++ toolchain and CMake:

```sh
export CMAKE_POLICY_VERSION_MINIMUM=3.5
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo test --locked --release --workspace
```

Nine Linux tests exercise:

- Actual Opus encode/decode, bounded buffers, seven-speaker mixing, duplicate and
  reordered frames, loss concealment, clipping and stream cleanup (two tests).
- Password authentication, friends/consent, conversation access, SQLite reopen,
  message idempotency, per-actor shortcut contributions, membership changes,
  session resume/logout, crossed friend requests, and protection from
  old-connection cleanup (three tests).
- An actual server subprocess with a generated local CA trust anchor, eight
  real TLS WebSocket clients, persisted and live messages, group invitations,
  eight-person membership, simultaneous senders, authenticated UDP relay,
  replay rejection, mute and independent deafen enforcement, malformed JSON,
  session reconnection, and TLS audio delivery without a UDP mapping. It also
  loads fifty maximum-length, JSON-escaped messages through the actual client
  receiver and verifies cursor pagination into the preceding history page. The same
  test runs the actual client networking engine through a transparent TCP
  proxy, forcibly cuts its connection, and verifies automatic session and call
  recovery with preserved controls and exactly-once persistence of a retried
  message (one integration test; TLS verification remains enabled).
- AEAD tamper/direction checks, authenticated replay windows and reordering,
  stale/truncated/oversized packets, strict request parsing and username
  validation (three tests).

The test runner's zero-test binary/doc targets are not counted as tests.
Synthetic PCM/Opus packets exercise real codecs and transport, not physical
microphone/speaker communication. Network failure injection intentionally
produces TLS unexpected-EOF log messages. These are expected during that test.

Windows source, including the DPAPI unit test, is cross-checked with Clippy
for all targets. The DPAPI test is **not run on Linux**. The committed Windows
CI job will run it when executed on a Windows runner; this session did not run
GitHub Actions or publish a release. The PowerShell packaging and benchmark
scripts require execution on Windows and were not run here. The Linux artifact
packaging path was executed locally.

## Before distributing as a validated release

- On clean Windows 10 and 11 x64, extract the complete portable package as an
  ordinary user and open the GUI. Verify registration/login/logout and protected
  session restoration after a restart. No compiler or browser may be required.
- Use a real hosted server with a valid certificate. Verify private chats,
  friend requests in both directions, group creation, oldest/newest page
  navigation, large Unicode messages, errors and retry states. Log in on another
  machine and check that the older client signs out without a reconnect battle.
- Test real WASAPI capture and playback with two computers and eight people,
  multiple speakers, headphones, independent microphone mute/headphone mute,
  device removal and reattachment, window responsiveness and leaving calls.
- Repeat on separate home networks. Block UDP, confirm TLS voice fallback,
  restore UDP, and test short outages/call rejoin. A last-person disconnection
  ends the call; a server restart ends all calls. Verify labels on two focused
  clients, late joins and simultaneous shortcut toggles.
- Run [resource benchmarks](BENCHMARKS.md), including a long call and repeated
  joins/leaves. Windows RAM, CPU, startup time, acoustic quality and long-run
  stability remain unmeasured; the <50/<100 MiB goals are not promises.

The protocol is functional but custom and unaudited. The server can read text
and voice. No end-to-end encryption, echo cancellation, password recovery,
background auto-update or signed installer is claimed.


## Oracle deployment update — 2026-10-27

The server now supports `--check-config` without opening a database/listener,
readable startup errors, and SIGTERM with an SQLite WAL checkpoint. The existing
real-network integration test checks valid/invalid configuration, no DB creation
during validation, clean SIGTERM exit and persisted database integrity.

Additional local verification:

- Six Python deployment tests: Bash syntax/help, both ELF architectures and
  SHA-256 manifests, non-mutating plans, invalid argument rejection, corrupted
  bundle rejection, firewall plan, JSON paths and the actual systemd directives.
  systemd-analyze verifies a temporary unit with local executable paths; it
  does not start systemd or install anything on the build host.
- ShellCheck for all `deploy/oracle/*.sh` scripts.
- Optimized x86_64 and aarch64 GNU/Linux builds. The ARM executable also runs the
  actual eight-client TLS/UDP integration test under QEMU user-mode, including
  session/call recovery and SQLite shutdown. Emulation is not an Oracle VM test
  or a performance measurement. `MINIVOICE_TEST_SERVER` can select a trusted
  wrapper executable when reproducing this integration check.

Commands from `mini-voice`:

```sh
python3 -m unittest discover -s tests/deployment -v
shellcheck deploy/oracle/*.sh
cargo test --locked --release --workspace
# Optional trusted wrapper which execs qemu-aarch64 -L SYSROOT ARM_SERVER "$@":
MINIVOICE_TEST_SERVER=/path/to/arm-server-wrapper cargo test --locked --release -p mini-server --test network
```

Not performed: Oracle account signup/billing eligibility, actual VM creation,
Oracle ingress and guest-firewall mutation, root installation on Ubuntu 24.04,
public ACME issuance/renewal or external internet call quality. The installer
is prepared and locally checked, not deployed. Verify those steps using
HOSTING.md before considering a hosted server ready. Oracle may require a
payment card even for free resources; this project provides no workaround.

## Standalone client TLS startup and background mode

The icon update rebuilt only `mini-client`, revealing a dependency on the
server's workspace features: `rustls` had no crypto backend in a standalone
client build. Auto-connect to a saved reachable server panicked during TLS
configuration; release `panic=abort` terminated the process. This was reproduced
by `cargo test --locked -p mini-client --test standalone_tls` before the fix.

The client now directly enables aws-lc-rs and installs its crypto provider before
spawning its connection task. The standalone test receives an actual TLS
ClientHello over loopback, without bypassing certificate verification. It passes
both alone and with the workspace. CI explicitly runs the standalone test before
workspace tests to prevent feature unification from hiding the regression.

The Windows-only `windows_startup` test launches the real client executable with
isolated settings and a saved local server endpoint. It verifies a TLS
ClientHello, window startup, close-to-tray with a live process, restoration via
the tray callback, minimize-to-tray, restoration again, Exit app, and clean
startup/exit logs. The optimized GNU Windows executable passed this test under
Wine 10 with Xvfb. This is compatibility-layer evidence; native Windows 10/11,
notification-area appearance, physical audio, actual microphone behavior while
hidden, and RAM/CPU still require testing on Windows. The existing eight-client
TLS/UDP/reconnect integration and all ten Linux Rust tests pass.

Startup diagnostics write bounded rotating logs to
`%LOCALAPPDATA%\MiniVoice\client.log` and report initialization errors or Rust
panics in a dialog. The log excludes panic payloads and does not record tokens,
passwords or messages. Windows loader failures/native exceptions before the
panic hook can still require Windows Event Viewer. Background mode keeps the
existing event loop/network/audio running; it does not install autostart or a
Windows service. If Shell refuses a notification icon, the window stays
accessible on the taskbar; Exit app remains available in the window.

## Audio / Profile and Enter-to-send update

The expanded Linux suite has seventeen tests: five client codec/gain/cache/settings
checks, the standalone TLS regression, six server account/profile/storage checks, an in-flight-removal race check,
three protocol/AEAD checks and the real eight-client TLS/UDP integration.
The integration now uploads through the actual client's bounded avatar channel,
receives normalized JPEG data, checks another participant's profile revision,
removes the avatar and verifies the removal synchronizes. Existing history,
reconnect, message idempotency, mute/deafen and hidden-label tests remain.

The mixer test compares decoded audio from two real Opus speakers and changes
one person's gain during playback. It checks independent gains, master silence,
mic zero gain and boosted-sample limiting. Preferences round-trip with defaults
and server/user-ID scoping. Cache tests enforce the twenty-thumbnail ceiling,
unchanged-revision reuse, replacement invalidation, immediate clearing when OFF,
and no decoding of stale or disabled replies. Server tests cover PNG/JPEG/WebP,
size/dimension rejection, ordered chunks, unrelated-user access denial,
replacement/removal, database reopen, two-upload bound and expired-buffer cleanup.

On Wine 10/Xvfb, `windows_startup` launches the actual optimized executable with
legacy settings and a saved TLS endpoint. It now also opens/closes the compact
panel, checks default device entries, changes microphone/master sliders, turns
Show Avatars off, saves, restarts and verifies all three preferences. The original
close/minimize-to-tray, restore and Exit app checks remain. The input-control test
checks Enter, held-key repeat suppression, Shift+Enter newline and simulated
IME composition/confirmation. Real IME applications are not exercised by these
synthetic Windows messages.

The existing DPAPI round-trip test was attempted under Wine and failed because
`CryptProtectData` returned failure in that compatibility environment. It has not
been claimed as passing or removed. Native Windows DPAPI, physical capture/
playback, actual device hotplug/switching, audio latency and Windows RAM/CPU
comparisons still need a Windows machine. The Windows CI job includes those
existing automated tests, but remote CI completion has not been verified here.
No numeric Windows performance improvements are claimed. Thumbnail pixel storage
has a strict 125 KiB ceiling, excluding transient image decodes and upload buffers.

The known CPAL 0.16 COM-activation unwrap is now preceded by an audio-service
preflight, so a missing backend does not abort merely when the panel opens.
No library/framework or runtime thread model was replaced. The only backend
extension is authenticated, size-bounded avatar operations on the existing TLS
connection and an additive SQLite table. See CLIENT-UPDATE.md for behavior,
limits and upgrade instructions.

## Voice cleanup / 40px square-avatar update

Two synthetic DSP tests measure settled output against 20 Hz rumble, 1 kHz
speech-band tones and 14 kHz hiss. They check rumble/hiss attenuation, low-level
noise reduction, preservation of a soft voice above the expander threshold,
exact bypass, mute reset, stable long silence, non-finite input handling and
fixed state below 128 bytes. The suite now contains nineteen Linux Rust tests.
The avatar-cache test checks 40×40 BGRA buffers and retains disabled/stale-reply
checks. Preferences and the Windows startup smoke test cover the cleanup default,
saving OFF and restoring it after restart.

No physical microphone listening test or native Windows CPU/RAM benchmark was
available. Synthetic frequency-response checks do not establish subjective voice
quality, echo cancellation or separation of loud noise from speech. Server and
wire formats are unchanged for this client-only update.
