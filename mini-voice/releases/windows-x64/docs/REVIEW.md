# Implementation review — 2026-10-27

The implementation covers accounts/sessions, mutual friends, private and small
group text conversations, persisted/paginated history, bounded message retries,
call invitations/membership, Opus capture/playback and mixing, authenticated UDP
with TLS fallback, independent mute/deafen, and authoritative call-local shortcut
state. Native Win32 source, Windows x64 executables, a Linux server, deployment
instructions and packaging/benchmark scripts are present.

This is **not yet a Windows acceptance-tested release**. No Windows execution,
physical audio test, Windows working-set/CPU/startup measurement, long call soak
or connection across separate residential networks was possible on this Linux
machine. The RAM targets remain unmeasured. The CI workflow is supplied but was
not run on GitHub in this session. Hosting still requires a reachable server
and a valid TLS certificate. See VALIDATION.md and HOSTING.md.

The second review corrected:

- A 128 KiB client receive limit which rejected valid history pages. Responses
  now have a bounded 512 KiB limit, sufficient for fifty 4 KiB messages including
  JSON escaping. An integration regression exercises the real client receiver
  with a full maximum-length page and its older-page cursor.
- Session persistence used the editable address field rather than the actual
  connection origin. Credentials now bind to the locally recorded connection
  endpoint; the address field is disabled while signed in.
- Crossed friend requests left a duplicate pending relationship. Acceptance now
  atomically keeps one accepted relationship and removes the reverse request.
  A regression checks both users' lists and repeated acceptance.
- Closing a WebSocket after a failed write had no timeout. Connection cleanup
  now bounds that close operation as well.

Validation commands are recorded in VALIDATION.md. Build caches, generated
archives, private certificates, local configuration and databases are ignored
by Git; source, lockfile, tests, license notices, documentation and CI belong
in the commit. Original Next.js/notebook files are unchanged.
