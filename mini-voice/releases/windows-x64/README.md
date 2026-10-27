# Mini Voice

A Windows 10/11 x64 native messaging and voice client, with a self-hosted
Windows/Linux server. No browser, Node, Python, hosted application platform,
or administrator rights are needed on friends' computers. The existing Next.js
project outside this directory is unrelated and unchanged.

**Status:** implemented and tested on Linux with eight real TLS clients and
UDP relay; Windows x64 builds are produced by cross-compilation. Windows GUI,
physical microphone/playback, internet NAT traversal and Windows RAM targets
require the acceptance checks below before calling this a validated release.
No public server is provisioned by this repository.

## Use

1. The host sets up the server using [deployment instructions](docs/HOSTING.md).
2. Extract the entire `mini-voice-windows-x64.zip`; open `mini-client.exe`.
3. Enter `wss://your-hostname:4433`, a username (3–24 ASCII letters, digits,
   underscore) and a password (10–128 UTF-8 bytes); click **Register**.
4. Enter your friend's username in the small field on the left; **Add friend**.
   Your friend selects `Request: username` and clicks **Accept friend**.
5. Select friends (Ctrl-click for a group), then **New chat**. Select the
   conversation below the friends list. Type plain text and click **Send**.
6. **Call** invites the conversation's members; others use **Accept call** or
   **Decline**. Select friends and **Invite to call** to add participants.
   Calls allow eight people. During calls the bottom bar contains microphone
   mute, independent headphone mute, and leave. Use headphones: echo cancellation
   and noise suppression are not implemented.

The server address is saved in `%LOCALAPPDATA%\MiniVoice\settings.json`.
The session token is protected by Windows DPAPI for the current Windows user.
Passwords are never saved. **Log out** revokes the current session. Sessions
expire after 30 days; one live client per account is supported. Signing in
elsewhere disconnects the older client. Pending messages are bounded and
retried with stable IDs during the same process lifetime; they are not saved
to disk and are lost if the client exits before acknowledgement. The status
line reports delivery, connection and server validation errors.

## Build

Developers need Rust (tested with 1.99.1), CMake and a C/C++ toolchain.
Packaging also uses Python 3.11+ to collect third-party license notices.
SQLite and Opus sources are built automatically; friends need no compiler.
Cargo.lock pins Rust dependencies. Bundled Opus 1.3.x needs the CMake
compatibility setting below with CMake 4; checksum verification stays enabled.

Windows: open a Visual Studio 2022 x64 Native Tools terminal with the Desktop
C++ workload and Windows SDK, install Rust's MSVC toolchain and CMake, then:

```powershell
cd mini-voice
$env:CMAKE_POLICY_VERSION_MINIMUM = '3.5'
cargo test --locked --workspace
powershell -ExecutionPolicy Bypass -File scripts/package-windows.ps1
```

The packaging script builds optimized MSVC executables and produces a portable
ZIP. No installer or autostart service is required. If MSVC reports a missing
runtime on a clean machine, install Microsoft's x64 Visual C++ 2015–2022
redistributable; GNU cross-builds instead include their required runtime DLLs.
Unsigned downloads may trigger Windows reputation prompts. We do not disable
SmartScreen; a release publisher can sign executables with their own certificate.

Linux server (GCC/Clang, CMake, make):

```sh
cd mini-voice
export CMAKE_POLICY_VERSION_MINIMUM=3.5
cargo test --locked --workspace
cargo build --locked --release -p mini-server
./target/release/mini-server server.json
```

The Linux client executable only explains that the GUI targets Windows; it is
not a Linux desktop client. Server-only builds do not need Opus or audio devices.

## Engineering and validation

[Architecture/security](docs/ARCHITECTURE.md) · [Deployment](docs/HOSTING.md) ·
[Tests and Windows acceptance](docs/VALIDATION.md) ·
[Implementation review](docs/REVIEW.md) ·
[Repeatable resource measurements](docs/BENCHMARKS.md)

RAM goals are **under 50 MiB idle / under 100 MiB calling**, not measured claims.
The client uses Win32 controls and system fonts, two Tokio workers, bounded
network/audio queues, 50-message history pages and a 200-message display cache.
Audio streams and codecs are released on leaving a call. No analytics or telemetry.
