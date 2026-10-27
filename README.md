# Mini Voice

Native Rust messaging and voice calling for Windows 10/11 x64, with a
self-hosted SQLite server. No embedded browser or hosted application platform.

Download the Windows builds: [client .exe](mini-voice/releases/windows-x64/mini-client.exe?raw=1),
[server .exe](mini-voice/releases/windows-x64/mini-server.exe?raw=1), or the
[complete portable ZIP](mini-voice/releases/mini-voice-windows-x64.zip?raw=1).

See [setup, builds and usage](mini-voice/README.md),
[hosting](mini-voice/docs/HOSTING.md), and the
[implementation review and remaining validation](mini-voice/docs/REVIEW.md).
Windows execution, physical audio and RAM/CPU targets still require acceptance
testing on Windows; Linux transport/codec tests do not establish those results.

The earlier Next.js application remains in `willownutstea/`; the NumPy exercise
notebook is independent of Mini Voice.
