<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 22. Base `origin/master` `87779be6` (K438 included), merged at
`e5f6d6e88`. Dedicated `kenbr0`/`kentap0`. The gate is not green: S5-session, S7's exit/byte gate
and S8 are red.

Run: `timeout 900 env RAVEN_QEMU_BRIDGE=kenbr0 RAVEN_QEMU_TAP=kentap0 sh tests/ssh/matrix.sh`
(exit 1, 6:46) and `timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | `K434-S0-ECHO-0123456789abcdef\n` echoed 30/30, 208 ms |
| S1 | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (1872 ms), control 1454 ms, client-first 1527 ms |
| S2 | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20` |
| S3 | PASS | wrong exit 255 + `Permission denied`; `Authenticated to ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | PASS | confirmation delivery PASS; identity qtest PASS |
| S5-session | **FAIL** | no EPIPE; exit -9 at 15 s; fd4 `socket:[271819680]`, `pipe:[271807916]`; TCP peer `ESTAB` |
| S6 | PASS | `receive packet: type 91` + `channel 0: open confirm rwindow 0 rmax 32768` |
| S7 | **FAIL** | 7/10 exact markers; attempts 7-9 exit 255 with 0 bytes (2155/8017/51 ms); the 7 byte-correct attempts exit -9 at ~30 s |
| S7-routing | PASS | client exact marker on the byte-correct attempts; `console_total_delta=0` |
| S8-pty-alloc | PASS | `pty-alloc: pass` |
| S8 | **FAIL** | `PTY allocation request failed on channel 0`; K438 merged but the staged `/bin/tty` is not reached |

## S7 exit/wall per attempt

```text
#1  bytes exact  exit -9  wall 30019 ms
#2  bytes exact  exit -9  wall 30030 ms
#3  bytes exact  exit -9  wall 30017 ms
#4  bytes exact  exit -9  wall 30020 ms
#5  bytes exact  exit -9  wall 30013 ms
#6  bytes exact  exit -9  wall 30013 ms
#7  0 bytes      exit 255 wall 2155 ms
#8  0 bytes      exit 255 wall 8017 ms
#9  0 bytes      exit 255 wall 51 ms
#10 bytes exact  exit -9  wall 30009 ms
```

S7-routing is PASS on the exact bytes and console delta 0; S7 is FAIL on the gate exit/bytes: 7/10,
no exit 0, walls 30 s.

## S8: K438 merged, PTY still fails

`pty-alloc` passes but `ssh -tt raven@... tty` still returns
`PTY allocation request failed on channel 0`. The test-only `/bin/tty` from
`tests/fixtures/ssh-tty` is staged and not reached. K438 (SCM_RIGHTS fd passing + received-socket
readiness) is on master; the SSH PTY path remains red.

## S5 identity

K431's fixture now stages `raven-consd`; the qtest passes (`socket-write-identity-tap: pass`). S5
identity and S5 PASS.

## Artifacts (SHA-256)

```text
6d29e90c9172dbdd5002c71f0e4aa0f306c9f11ae4c937a1c513ffd0cc3b61e8  build/ssh/matrix/matrix.tsv
a5fa3b9d716660fa4f692118960832b36756bea861ff26fe0b447ec476440e95  build/ssh/matrix/attempts.tsv
0660813a567584e32c39f7ce869f993858e156c95c8d09cec5e1bd541854a86e  build/ssh/matrix/serial.log
8b3a4ee20512bf06991be0f154dfa74b162f6348b94042f8216fd5c28c43e5db  build/ssh/matrix/serial-echo.log
882b7b17640f88fdb8e9b1aeea49ab60ed7fac3b4a221ec7b6f5d4abd4d4dbf5  build/ssh/matrix/s7-marker.log
2cd4b0dabf4b992e5e52498ac2831666a3426b3a240579d36d91d7e3ed732c71  build/ssh/matrix/s8-tt.log
a31507a51bc4c0314d1f1f6331416a2e2e252bde9ba9ccdba7daa02e770cc8d7  build/ssh/matrix/pty-alloc.log
c9fb91aa40753bc16093d1a1b34b994bbb80bb95514577e02dc1813c5c0b3ba4  build/ssh/matrix/s5-vvv.log-probe.log
b3f98d503fbc69ec66b9659c7ec65bf3fe62f7a1e748f97d0be380635d2fe013  build/ssh/matrix/s5-vvv.log-strace.log
a1e776d3be284f3dd42735f49228c2057c4461529702a55b1c259ed8fe326d69  build/ssh/matrix/socket-write-identity.log
9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa  build/ssh/matrix/socket-write-identity.exit
75df17543022ac1d9b2cd711436f2fe1524312597d7d89a8fb09c4bf18ec23be  matrix run log
```

## Open gaps

- S7 gate: 7/10 exact; no exit 0; 30 s walls; three attempts exit 255/0 bytes.
- S5-session: no EPIPE; exit -9 at 15 s; fd4 socket/pipe; TCP peer ESTAB.
- S8: PTY allocation still fails with K438; test-only tty not reached.
- S6 length: not in the client log.
- S3 peer address: `from UNKNOWN port 65535`.
