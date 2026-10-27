# goyvoice Linux ARM64 server

Optimized Rust 1.99.1 aarch64-unknown-linux-gnu cross-build, for Ubuntu 24.04
on ARM64/Ampere VM shapes. Requires glibc 2.38 or newer.

Follow [Oracle installation](../../docs/HOSTING.md). Architecture and SHA-256
are checked by the installer. Windows friends use the same x64 client; only
the server runs on ARM. --check-config and systemd SIGTERM are supported.

This binary ran the real eight-client TLS/UDP integration test under local
QEMU user-mode emulation, including SQLite persistence, reconnect and shutdown.
That is not validation on an Oracle VM, its network or a public certificate.
Keep LICENSE and THIRD-PARTY-NOTICES.txt when redistributing.

This update includes persistent friend-visible voice rooms with live occupancy
counts. Default creator: judas; creation PIN: 231939. Update Windows clients too;
see [room guide](../../docs/ROOMS.md). Configuration keys are optional, so existing
Oracle server.json files continue to work.
