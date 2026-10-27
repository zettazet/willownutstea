# Mini Voice Linux x86_64 server

Optimized Rust 1.99.1 GNU/Linux x86_64 build; Ubuntu 24.04 target deployment.
Requires glibc 2.38 or newer. Not compatible with ARM/Ampere machines.

Follow [Oracle installation](../../docs/HOSTING.md). The installer verifies
BINARY-SHA256SUMS.txt and installs this binary as an unprivileged systemd service.
No Rust compiler is needed on the VM. --check-config validates local JSON and
TLS key material without opening listeners or a database. SIGTERM checkpoints
SQLite before exit. Protocol remains compatible with the existing Windows client.

Local network/codec tests passed; no live Oracle deployment/ACME issuance is
claimed. Keep LICENSE and THIRD-PARTY-NOTICES.txt when redistributing.
