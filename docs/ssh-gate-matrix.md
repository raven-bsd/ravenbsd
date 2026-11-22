<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 21. Base `origin/master` `e313c1df` (K431/K436/K437/K439/K433 +
MPTCP), merged at `0a2069dea`; final code `3d6f4d43e`. Dedicated `kenbr0`/`kentap0`. The gate is not
green: S5-session, S7's exit/wall assertion and S8 are red.

Run: `timeout 900 env RAVEN_QEMU_BRIDGE=kenbr0 RAVEN_QEMU_TAP=kentap0 sh tests/ssh/matrix.sh`
(exit 1, 8:27) and `timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | `K434-S0-ECHO-0123456789abcdef\n` echoed 30/30 bytes, 223 ms |
| S1 | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (6776 ms), control 2026 ms, client-first 2220 ms |
| S2 | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20` |
| S3 | PASS | wrong exit 255 + `Permission denied`; `Authenticated to ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | PASS | confirmation delivery PASS; identity qtest PASS (`socket-write-identity-tap: pass`, exit 0) |
| S5-session | **FAIL** | no EPIPE; exit -9 at 15 s; fd4 `pipe:[270353297]`; TCP peer `ESTAB` |
| S6 | PASS | `receive packet: type 91` + `channel 0: open confirm rwindow 0 rmax 32768` |
| S7 | **FAIL** | marker exact 11/11; console delta 0; all ten gate attempts exit -9 at ~30 s; walls `[30010, 30038, 30030, 30022, 30032, 30002, 30019, 30022, 30033, 30031]` ms |
| S7-routing | PASS | client exact marker; `max_console_new=0`; `console_total_delta=0` |
| S8-pty-alloc | PASS | `pty-alloc: pass` |
| S8 | **FAIL** | `PTY allocation request failed on channel 0`; K438 pending; the test-only `/bin/tty` is not reached |

## S5 identity fixture (fixed in K434 Owns)

K431's identity fixture was missing `consd` in its static `server` image, so `rsh` failed with
`rsh: tty:console: no such file or directory`. K434 stages `raven-consd` and a consd service in
`tests/fixtures/socket-write-identity/build-ssh.sh`. The test now passes:

```text
timeout 240 sh tests/fixtures/socket-write-identity/run-tap.sh   exit 0 (37.6 s)
socket-write-identity-tap: pass
```

S5-identity and S5 are PASS.

## S7 exit/wall assertion

The client receives the exact marker and the console receives none, but the process does not exit.
Each of the ten attempts is killed at the 30 s cap:

```text
S7-exit #1  FAIL  exit -9; wall=30010 ms; bytes=K434-SSH-MARKER-2026-11-22\n
S7-exit #2  FAIL  exit -9; wall=30038 ms; bytes=K434-SSH-MARKER-2026-11-22\n
...
S7-exit #10 FAIL  exit -9; wall=30031 ms; bytes=K434-SSH-MARKER-2026-11-22\n
```

S7-routing is PASS on bytes/routing; S7 is FAIL because the gate requires exit 0 and wall <= 5000 ms.

## S8

`pty-alloc` passes; `ssh -tt raven@... tty` still returns
`PTY allocation request failed on channel 0`. K438 (SCM_RIGHTS) is the owning fix and is not on
master. The test-only `/bin/tty` from `tests/fixtures/ssh-tty` is staged but not reached.

## Artifacts (SHA-256)

```text
a92f59fd44bbdf24023ce02fd07d43314815c7afa5537164f79f5aeb6eace5ff  build/ssh/matrix/matrix.tsv
4460965e6471c3c375c4fbfd7796eba0db79d59b12120bf31635731f6ce093d2  build/ssh/matrix/attempts.tsv
b7827cd92dd4db2b57557fc5fac43bd5fc86ae31baef359c2e7c9be1df203f22  build/ssh/matrix/serial.log
d35027f050a470a240546ff6a821bd68ffd3b2889ede63a46ca5e204356905a0  build/ssh/matrix/serial-echo.log
6cb6c40a9cce4153e8545efb82a3360c2b42165df4cd091337a7f3bdc0182c63  build/ssh/matrix/s7-marker.log
bba9c082f95bd771759f7b4b13ab89b34862bd55b592f91243d018e6c874823c  build/ssh/matrix/s8-tt.log
c981a3d9d2e5ba35ae3371b8c6ca07c76374b3f2d7ac31d25a0a4e112287f827  build/ssh/matrix/pty-alloc.log
90b1e0ae1509e70e3e4989ba33389b0221aa681a5da14ae1985e4c26987aa900  build/ssh/matrix/s5-vvv.log-probe.log
68b6b73481e05bad739e15db8b99ac9c2877812c5ed4cd294e4724f8ca2d66b2  build/ssh/matrix/s5-vvv.log-strace.log
a1e776d3be284f3dd42735f49228c2057c4461529702a55b1c259ed8fe326d69  build/ssh/matrix/socket-write-identity.log
9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa  build/ssh/matrix/socket-write-identity.exit
ce426b92cec552d76368bcabb0dd8ba9a130636c858b522af4c1e1cebe14a9e6  matrix run log
```

## Open gaps

- S7 gate exit: 10/10 exact bytes and console delta 0, but exit -9 at 30 s; wall times 30.0-30.05 s.
- S5-session: no EPIPE; exit -9 at 15 s; fd4 pipe/socket; TCP peer ESTAB.
- S8: PTY allocation fails; K438 SCM_RIGHTS pending.
- S6 length: not observable from the client log.
- S3 peer address: `from UNKNOWN port 65535`.
