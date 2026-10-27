# Architecture and security

`shared` defines size-limited JSON signaling and the UDP AEAD envelope;
`server` owns SQLite, authentication, subscriptions and call state;
`client` separates Win32, DPAPI settings, async networking and audio.

Win32 was selected over Slint to avoid a rendering toolkit, GPU backend and
asset/font bundle. Controls use the Windows system font. No browser process
is launched. Native controls have dark content regions with standard Windows
button chrome. There are no animations, images, avatars or splash screen.

## Text and accounts

A single TLS WebSocket per client carries authentication, plain text messages,
call invitations and authoritative snapshots. Passwords go through TLS, then
Argon2id (19 MiB, two passes, one lane, random salts). Expensive password work
runs on a bounded blocking pool outside the state mutex, so login does not
hold up UDP relay. Never deploy a plaintext reverse-proxy hop instead of TLS.

Session tokens have 256 random bits; SQLite stores only SHA-256 token hashes.
They expire in 30 days and logout revokes the current token. A login replaces
that account's earlier live connection. Session generations prevent an old
socket from cleaning up the new connection. Windows DPAPI protects local tokens;
other software running as the same Windows user remains in the trust boundary.

SQLite uses WAL, foreign keys and a conversation/id history index. Membership
is checked on every history query, send and call creation. A unique
(sender, nonce) key makes retries idempotent; reusing a nonce with different
content is rejected. History is paginated by message ID, 50 per page. Clients
load only the selected conversation and retain at most 200 displayed messages.
Friends require acceptance; group creation requires accepted friendships with
the creator. Invitees may participate in a call without gaining access to its
conversation's stored messages. Groups have up to eight members.

Limits: 128 simultaneous sockets, two parallel Argon2 jobs, 20 connection
attempts per IP per minute, five authentication attempts per connection,
120 JSON operations per connection/minute, 60 voice frames/s/sender,
32 KiB client request frames, 512 KiB bounded server response frames (including
JSON-escaped history pages), 4 KiB message bodies, 128 friend relationships,
128 conversations per creator, 128 active calls. TLS and authentication have
10s timeouts, writes have 5s timeouts, idle sockets expire after 35s. Overloaded
reliable event queues force reconnect; audio queues drop excess frames.
For larger or hostile deployments use firewall limits and registration
restrictions; this is designed for a small private group, not a public service.

## Audio

WASAPI via CPAL opens the default microphone and playback device only while
calling. Float and common integer sample formats are converted to mono 48 kHz.
Capture uses fixed 960-sample frames (20 ms). Opus VoIP at 24 kbit/s, complexity
3, sends independent speakers; the client mixes up to seven incoming streams.
Capture hands fixed-size packets to a three-frame queue without allocating
sample/packet buffers in the callback. Playback uses try_lock and outputs silence
instead of blocking the audio thread. Device errors release streams and leave
the call, allowing a later retry. The simple sample-rate converter is linear
on capture and sample-hold on playback; high-quality resampling, AEC, AGC and
noise suppression are not implemented. Headphones are strongly recommended.

Authenticated UDP probes establish each recipient's NAT mapping and keep it
alive once per second. Upload packets contain call ID, sequence and Opus bytes.
The server validates membership and microphone state, then forwards only to
non-deafened participants, including sender and sequence. Call IDs prevent late
packets from one call leaking into another. Each receiving stream has a bounded
six-frame (120 ms maximum) jitter buffer, starts after two frames, reorders
packets, discards duplicates/late packets and uses up to three frames of Opus
packet-loss concealment. Large gaps re-prime the buffer. Voice allocation occurs
on the async transport/initialization paths; fixed buffers are reused in the
real-time callbacks. No voice is written to disk.

UDP uses ChaCha20-Poly1305: random per-connection 256-bit secret delivered over
TLS, SHA-256-derived separate upload/download keys, a fresh random 128-bit session
ID, monotonic 64-bit nonces and an authenticated session/sequence header. A
64-packet replay window allows reordering. Failed authentication cannot consume
a valid sequence. Packets fit in a 1400-byte receive buffer. This is a custom
transport, not an independently audited protocol. The server decrypts/relays
voice and stores text; **this is not end-to-end encryption**.

When UDP acknowledgements stop for three seconds, voice uses binary frames on
the existing TLS WebSocket. UDP resumes when probes work again. TCP fallback
can suffer head-of-line latency on lossy links. Outbound NAT mappings avoid
requiring client-side port forwarding; the server still needs reachable TCP
and UDP ports. This was tested locally, not across residential ISPs.

Text automatically reconnects with bounded exponential delay (1–15s) and
resumes its token. Message retries retain their original nonce. For brief
connection loss a disconnected participant is removed from displayed membership;
the server reserves a rejoin invitation for 30 seconds while others remain.
The client automatically rejoins that call on session recovery, restoring mute
and deafen. A call ends when its last participant leaves/disconnects. Expired
reservations, declined invitations, explicit leave and ended calls cannot rejoin.
Server restart destroys transient calls; stored chats/accounts remain.

## Call-local shortcut state

The server stores a set of activating participant IDs per call. A participant
is labeled exactly `gay` iff at least one other current participant has an
active toggle. An actor's own activation never labels that actor; another
actor's activation may. Each actor toggles only their own contribution. Joins
receive a complete snapshot; departure removes that actor's contribution.
Call destruction resets everything. This state is never stored in SQLite.
Alt+V is handled only by the focused Windows message loop, without key-repeat
retriggering or a global keyboard hook. No interface button or hint exposes it.

## Boundaries

No password recovery, account deletion UI, key rotation service, remote update,
audit service, file attachments or third-party platform dependencies. Server
operators can read messages and relay audio. Protect host accounts, certificate
keys and database backups. No independent security audit has been performed.
