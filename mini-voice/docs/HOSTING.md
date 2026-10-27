# Host a server

Friends need a reachable host: a small Linux VPS, Windows computer or home PC.
No hosted application platform or Docker is required. A commercial domain or
VPS is optional; an existing hostname and reachable home connection can work.
The server is a normal process and does not install background services.

## TLS

Use a hostname pointing to the server's public IP. Obtain a public CA certificate
with an ACME client (for example Certbot on Linux, win-acme on Windows). With
Certbot's standalone HTTP challenge, temporarily allow inbound TCP 80 and run
`certbot certonly --standalone -d voice.example.org`. DNS-01 is an alternative
when port 80 is unavailable. Follow the CA client's instructions for renewal.

Copy the complete certificate chain and private key to readable files for the
server's dedicated unprivileged account; restrict private-key permissions
(`chmod 600 privkey.pem` on Linux, equivalent user-only NTFS ACL on Windows).
Keep the server data directory private (`chmod 700` on Linux).
Use `fullchain.pem`, not only the leaf certificate. Never send the private key
to friends or commit it. After certificate renewal, restart the server to load
the new files. An external process manager may do this; active calls then end.

For a private CA, explicitly install its CA certificate in each friend's
Windows **Current User → Trusted Root Certification Authorities** store, after
verifying its fingerprint out of band. The hostname must still match the
certificate SAN. The client never ignores invalid certificates. Public CA
certificates avoid this manual installation.

## Configure and start

The supplied Linux x64 binary requires glibc 2.38 or newer (for example Debian
13 or Ubuntu 24.04). Build from source on older hosts. The Windows binaries
require Windows 10/11 x64.

Copy `server.example.json` to `server.json` beside the server executable.
Edit database and certificate paths (relative paths resolve from the working
directory). Keep UDP's advertised port identical to the externally forwarded
UDP port. Start from that directory:

```sh
./mini-server server.json
```

Windows PowerShell: `./mini-server.exe server.json`. Friends connect to
`wss://voice.example.org:4433` and register their own accounts.
Allow inbound **TCP 4433 and UDP 4434** in the host firewall. Ordinary use of
the client needs neither elevation nor inbound firewall rules. Firewall changes
on a server may require administrative privileges.

## Home hosting

Reserve the host's LAN IP in the router. Forward external TCP 4433 → host TCP
4433 and UDP 4434 → host UDP 4434. Keep the advertised UDP port the same. Add a
DNS A record for the WAN IPv4 address; update it with dynamic DNS if it changes.
If publishing AAAA too, ensure IPv6 connectivity/listening/firewall works;
otherwise omit the AAAA record. Test with a friend on a different network or
cellular data, not only your own Wi-Fi (NAT loopback may not work).

CGNAT/double NAT can prevent incoming connections even with router rules. Ask
the ISP for a public address, forward through both routers when you own them,
or use a VPS. The application does not tunnel through CGNAT. Do not disable
TLS to work around hostname/certificate errors. If UDP is blocked, messaging
and voice over TLS should still work, with potentially higher call latency.

## Small VPS

Create an unprivileged `mini-voice` user, copy the Linux binary and configuration
into a directory owned by it, issue a certificate and open the two ports in
both cloud and OS firewalls. A 512 MiB–1 GiB VPS is a reasonable starting point
for a private group, not a measured capacity promise. Argon2 login bursts use
roughly 19 MiB per concurrent hash (at most two), beyond idle server usage.

For optional systemd supervision, use an ordinary service with
`User=mini-voice`, `WorkingDirectory=/opt/mini-voice`,
`ExecStart=/opt/mini-voice/mini-server /opt/mini-voice/server.json`,
`Restart=on-failure`, `NoNewPrivileges=true`. Check file ownership and permissions.
No container is needed. Avoid exposing any database or private key through a
web server.

Back up using SQLite's online backup API/CLI `.backup` command, or stop the
server and copy the database together with any WAL/SHM files. Do not copy only
the main database while WAL writes are active. Monitor free disk space; chat
history has no automatic retention policy. Restore into a stopped server.

## Acceptance across the internet

Register two distinct accounts on separate networks; accept a friend request;
exchange messages; restart both clients and verify history/session restore;
call in both directions. Test eight users, simultaneous speakers, independent
mute/deafen, then block UDP temporarily and confirm TLS voice fallback. Unblock
UDP and confirm recovery. Interrupt one client's internet briefly and confirm
call rejoin while the other stays connected. Repeat with headphones and a USB
audio device disconnection. LAN-only tests do not establish these outcomes.
