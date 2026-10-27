# Validation and remaining acceptance work

Linux build environment: Debian 13 x64, Rust 1.99.1, GCC, CMake 4.4.4.
Windows cross target: x86_64-pc-windows-gnu, GCC MinGW-w64 14, release profile.
The Windows runtime, physical audio devices and separate residential networks
are unavailable here. An executable being built is not proof it runs correctly
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
