# Resource measurements

**Windows results are unmeasured.** There is no Windows runtime or physical
audio device in the Linux build environment. Goals: client <50 MiB idle,
<100 MiB calling. Executable size and Linux tests are not Windows RAM evidence.

Use an optimized Windows build on both an older Windows 10 x64 computer and
Windows 11. Record CPU, logical core count, physical RAM, OS build, binary SHA-256,
audio device/sample rate, network path/transport, and enabled security software.
Do not compare debug builds with another app's release builds.

1. Startup, with the client closed:
   `./scripts/benchmark.ps1 -Exe ./mini-client.exe -Scenario idle -Seconds 120`.
   The startup metric is first Windows input-idle, **not** login/network readiness.
   Repeat cold and warm launches five times and report median/range.
2. Leave a logged-in client open without messages/calls for 60s; find its PID
   with `Get-Process mini-client`. Run
   `./scripts/benchmark.ps1 -ProcessId 1234 -Scenario idle -Seconds 300`.
3. Exchange at least 100 messages, scroll a 200-message window, request older
   pages, switch conversations. Sample 300s with `-Scenario messaging`.
4. Have two physical clients join a call, both speaking intermittently into
   real microphones. Sample 600s with `-Scenario call2`.
5. Repeat with eight participants, then simultaneous speech from all eight;
   sample 600s with `-Scenario call8` and repeat for 1–2 hours to find growth.
6. Repeat call tests with UDP blocked, and on a lossy internet path. Note
   buffering, intelligibility, discontinuities and CPU alongside memory.
7. Leave the call; after 60s verify memory stabilizes near idle, audio streams
   are released and idle CPU recovers. Repeat 20 joins/leaves and device removal.

The script records working set, private bytes and CPU deltas every second;
CSV files contain scenario and UTC timestamps. Working set is resident RAM;
private bytes is committed private memory. Report both mean/p95/peak resident
RAM and private-byte trend, not a single Task Manager screenshot. One-core CPU
percentage is normalized to one logical core (100% = one core); whole-machine
percentage divides by core count. Keep background workloads constant. The
script never claims audio or network success from a PID alone.

For the Linux server, measure RSS/private memory separately with `/usr/bin/time
-v`, `pidstat -r -u -p PID 1`, or `/proc/PID/status`. Include startup, two concurrent
logins (Argon2 peaks), eight-person simultaneous speech, message pagination
and reconnect storms. Persistent database size is not process RAM. No server
capacity or Windows performance promises are inferred from a short smoke test.
