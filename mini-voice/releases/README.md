# Windows x64 downloads

- [mini-client.exe](windows-x64/mini-client.exe?raw=1) — desktop client.
- [mini-server.exe](windows-x64/mini-server.exe?raw=1) — self-hosted server.
- [Complete portable ZIP](mini-voice-windows-x64.zip?raw=1) — both executables,
  license notices, sample server configuration and documentation.

Optimized GNU Windows x64 builds from the Rust sources in commit `9a26b44`.
Only Windows system DLL imports are required. Friends do not need Rust, Node,
Python or a browser. The server needs a reachable host and a valid TLS
certificate; see [hosting instructions](../docs/HOSTING.md).

Checksums: [executables](windows-x64/BINARY-SHA256SUMS.txt) and
[ZIP](mini-voice-windows-x64.zip.sha256).

These unsigned builds were cross-compiled on Linux. Windows execution,
physical audio and Windows RAM/CPU measurements remain unverified; see
[validation status](../docs/VALIDATION.md). Keep the accompanying license
notices when redistributing these binaries.
