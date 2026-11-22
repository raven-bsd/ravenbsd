<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 23. Base `origin/master` `87779be6` (K438), code `631002516`.
S1-repeat is isolated on its own boot (one TCP readiness probe, then ten fresh attempts), followed by
a fresh main boot. The gate is red: S5-session, S7's exit/wall assertion and S8.

Run: `timeout 900 env RAVEN_QEMU_BRIDGE=kenbr0 RAVEN_QEMU_TAP=kentap0 sh tests/ssh/matrix.sh`
(exit 1, 10:32.9) and `timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | `K434-S0-ECHO-0123456789abcdef\n` echoed 30/30, 1207 ms |
| S1 | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (3246 ms), control 1769 ms, client-first 1305 ms |
| S1-repeat | PASS | 10/10 fresh connections: `Remote protocol version 2.0, remote software version OpenSSH_10.6` and `Authenticated to ... using "password"`; walls 8014-8027 ms; exits -9 after auth |
| S2 | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20` |
| S3 | PASS | wrong exit 255 + `Permission denied`; `Authenticated to ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | PASS | confirmation delivery + identity qtest PASS |
| S5-session | **FAIL** | no EPIPE; exit -9 at 15 s; fd4 `socket:[272078391]`, `pipe:[272078389]`; TCP peer `ESTAB` |
| S6 | PASS | type 91 + `channel 0: open confirm rwindow 0 rmax 32768` |
| S7 | **FAIL** | 10/10 exact markers; console delta 0; all ten exit -9 at 30.0-30.05 s |
| S7-routing | PASS | client exact; `max_console_new=0`; `console_total_delta=0` |
| S8-pty-alloc | PASS | `pty-alloc: pass` |
| S8 | **FAIL** | `PTY allocation request failed on channel 0`; test-only `/bin/tty` not reached |

## S1-repeat detail

The ten attempts are on a separate boot. A single TCP readiness probe is done first (the serial
`svcd: sshd up` line precedes the listener bind); the ten attempts themselves are fresh.

```text
#1   step=ok  banner=...OpenSSH_10.6  auth=Authenticated...  exit=-9  wall=8021 ms
#2   step=ok  ... exit=-9  wall=8017 ms
...
#10  step=ok  ... exit=-9  wall=8016 ms
connect_fail=0 banner_fail=0 auth_fail=0 ok=10
```

The row asserts the exact banner/auth lines; the exit -9 is the session-completion failure
(S5-session/S7-exit), not the banner/auth step.

## S7 exit/wall

```text
#1-#10  bytes=K434-SSH-MARKER-2026-11-22\n  exit -9
        walls=[30047, 30010, 30025, 30041, 30026, 30038, 30018, 30038, 30017, 30015] ms
```

S7-routing is PASS on bytes/routing; S7 is FAIL because the gate requires exit 0 and wall <= 5000 ms.

## S5-session

No EPIPE. `exit -9`, `timed_out=True` at 15 s. fd4 `socket:[272078391]`,
`pipe:[272078389]`; TCP peer fd 3 `ESTAB 10.0.2.2:45358 -> 10.0.2.15:22`; server handle not exposed.

## S8

`pty-alloc` passes; `ssh -tt raven@... tty` still returns
`PTY allocation request failed on channel 0`. K438 is merged; K442 is next. The test-only `/bin/tty`
is staged and not reached.

## Artifacts (SHA-256)

```text
f89991cc137c80a000857f02ce33b44e05a5e4fb089c274e66383c28ed3f7d92  build/ssh/matrix/matrix.tsv
0e61a8ceeb281a9d82de586b01317e2f6eddf0519e7eaa5b8db612de18fd7b62  build/ssh/matrix/attempts.tsv
a21702f8b6dce7284efff52a75a6a4d297398cbf0616800cd8fe61efbe6fedbb  build/ssh/matrix/serial.log
6386420a68d02ea768b2db41ad99ed9d74b367febb0cb18188d430557d92c38e  build/ssh/matrix/serial-repeat.log
3883aa9aab8ac0ac3fb35af94d29ab12cec89b87a8ba2ce7de3568af08b2a91c  build/ssh/matrix/serial-echo.log
d19b07eeebd784248353320e3b4327d9d5406ebafe53d34e987730a636d94342  build/ssh/matrix/s7-marker.log
1d499fef43f62a43b3c31503d4fe1f60a623bbc73369e20c692674814026e80b  build/ssh/matrix/s8-tt.log
1a552b8561e8a5dc641aa813c3c6301296964efc29c9b4ad496cad03ddd5e39b  build/ssh/matrix/pty-alloc.log
d2f0421dc7da792e3279ac4322ba7eb879b8b5843d60fc2ec85d6aa51e3229ef  build/ssh/matrix/s5-vvv.log-probe.log
61d0a3ad7e8358d8670abfd48e3c2b5361a25b1ebf3d0de83693a476b05ca712  build/ssh/matrix/s5-vvv.log-strace.log
a1e776d3be284f3dd42735f49228c2057c4461529702a55b1c259ed8fe326d69  build/ssh/matrix/socket-write-identity.log
4ed01d4216e5807c6ecc5671e1e5054918551a3b32af5ea68917db12ce6898e0  matrix run log
```

## Open gaps

- S7 gate: 10/10 exact, but exit -9 at 30 s; no exit 0.
- S5-session: no EPIPE; exit -9 at 15 s; fd4 socket/pipe; TCP peer ESTAB.
- S8: PTY allocation still fails after K438; K442 next.
- S6 length: not in the client log.
- S3 peer address: `from UNKNOWN port 65535`.
