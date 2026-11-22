<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 20. Base `origin/master` `1f2c0e64` (K431, K436, K437, K439,
K433), merged into `ken/K434` at `340500a7d`; final code `f3712ad01`. Dedicated bridge/tap
`kenbr0`/`kentap0`. The matrix is not green: S5, S5-session and S8 are red.

Run: `timeout 900 env RAVEN_QEMU_BRIDGE=kenbr0 RAVEN_QEMU_TAP=kentap0 sh tests/ssh/matrix.sh`
(exit 1; 3 red summary rows) and `timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Step | Test | Expectation (source) | Result | Exact assertion / artifact |
|---|---|---|---|---|
| S0 | guest TCP accept/echo over tap | BSD `accept`/`read`/`write` (C1) | PASS | 30/30 bytes `K434-S0-ECHO-0123456789abcdef\n`, 251 ms |
| S1 | banner, fresh + control | OpenSSH `kex.c:1297`; `version.h:3` | PASS | `SSH-2.0-OpenSSH_10.6\r\n` (1118 ms), control 927 ms, client-first 929 ms |
| S2 | KEXINIT | RFC 4251 section 6 | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20` |
| S3 | auth | OpenSSH `auth.c:264`, `auth-passwd.c:79` | PASS | wrong exit 255 + `Permission denied`; client `Authenticated to ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | post-auth read | POSIX `read`; FreeBSD `soreceive` | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | confirmation + identity | POSIX `write`; FreeBSD `sosend`; K431 fixture | **FAIL** | confirmation delivery PASS; identity fixture exit 1, `rsh: tty:console: no such file or directory` |
| S5-session | post-exec session | POSIX `read`/`write`; C10 | **FAIL** | no EPIPE; exit -9 at 15 s; fd4 `socket:[270179354]`, `pipe:[270155734]`; TCP peer `ESTAB 10.0.2.2:51364 -> 10.0.2.15:22` |
| S6 | confirmation | RFC 4254 section 5.1 | PASS | `receive packet: type 91` + channel 0 |
| S6-length | 44-byte plaintext | `channels.c:2198`/`:5085` | BLOCKED | not in the client log |
| S7 | marker + 10/10 | gate C1-C10 | PASS | client received exactly `K434-SSH-MARKER-2026-11-22\n` on the single attempt and all ten; exit -9 after delivery is S5-session |
| S7-routing | client receives, console does not | C5/C9/C10 fd routing | PASS | client exact marker; `max_console_new=0`; `console_total_delta=0` |
| S8-pty-alloc | dynamic pty case | K430/K436 libc test | PASS | `pty-alloc: pass` |
| S8 | PTY | POSIX `openpty`; OpenSSH `pty.c` | **FAIL** | `PTY allocation request failed on channel 0`; the test-only `/bin/tty` was not reached |

## S5 identity fixture failure

K431's `socket-write-identity` qtest is now on master and was run directly:

```text
timeout 240 sh tests/fixtures/socket-write-identity/run-tap.sh   exit 1 (1:47)
socket-write-identity: accepted fd=1
socket-write-identity: accepted ino=5
rsh: tty:console: no such file or directory
socket-write-identity: shell status=1
init exited (1)
```

The identity bytes were not reached; the failure is the fixture's shell opening `tty:console` in
the `server` profile, which has no consd service. This is recorded as S5-identity FAIL.

## S7 routing (K439)

The exact bytes now arrive on the client channel:

```text
S7-marker     PASS  K434-SSH-MARKER-2026-11-22\n; console_new=0
S7-10/10      PASS  10/10 exact marker; console_new=0 each
S7-routing    PASS  client=K434-SSH-MARKER-2026-11-22\n; max_console_new=0;
                    console_total_delta=0
```

The client still exits -9 after delivering the output; that completion failure is S5-session, not the
marker or its routing.

## S8 PTY

`pty-alloc` passes. `ssh -tt raven@... tty` still returns
`PTY allocation request failed on channel 0`. A test-only `/bin/tty` is staged from
`tests/fixtures/ssh-tty` so the exact command can print `/dev/pts/N` if allocation succeeds; the
failure occurs before the command runs.

## Artifacts (SHA-256)

```text
fdffa0137620231c96e10b5a2d04281f629e420da57b6a9d62dcaee706ea30e8  build/ssh/matrix/matrix.tsv
a606b3ce23d9b647be7bd0386ea10ec14854c1ed2ece8e6cd075246bcbfa4b7b  build/ssh/matrix/attempts.tsv
6b340e694f7df9fcdb12c89ebc6cfb1c2ef224a4a01a14b2cb939c05ea8e04a0  build/ssh/matrix/serial.log
cd3565b570aa6ce61e3811dddef645c689f6186f4d589b9e1ff2e710793ba85f  build/ssh/matrix/serial-echo.log
cd3c9bb261545920b949e15436dc00a0f72ad76fdff7d2b04ed965a66c6b3646  build/ssh/matrix/s7-marker.log
a0f99dde6ee41820b438aedfb764b0e26b06e358149085fbafc6d384b1f0f47b  build/ssh/matrix/s8-tt.log
6c7dac5049f925d37920908e41c28c144faf07146526b92ddc86cc2e3d6da4e9  build/ssh/matrix/pty-alloc.log
2e0988ec0b2be29f71b87553562da4c57f6bfe73f1b6e324177b581421c4ea0e  build/ssh/matrix/s5-vvv.log-probe.log
5af00dc9e0eb5e2ba12417ac461c71107b328a706f817629b7def216a5e445f0  build/ssh/matrix/s5-vvv.log-strace.log
b213ecf60a83c873029caed1b98feb40443a764d2d916d64a5397eaf0471bddf  build/ssh/matrix/socket-write-identity.log
4355a46b19d348dc2f57c046f8ef63d4538ebb936000f3c9ee954a27460dd865  build/ssh/matrix/socket-write-identity.exit
2e4dd0b43e6dbc47697c10950abdfa42c6b3ef85364944ea0ab9f411db243a9b  matrix run log
```

The K434 report carries gates, provenance and full open gaps.

## Open gaps

- S5 identity: K431's fixture fails at `rsh: tty:console: no such file or directory` before the
  identity bytes.
- S5-session: no EPIPE; the client does not complete after exec (exit -9 at 15 s); fd4 socket/pipe,
  TCP peer ESTAB, server-side handle not exposed.
- S6 length: the 44-byte plaintext length is not observable from the client log.
- S8: PTY allocation still fails after K436; the test-only `tty` is staged but not reached.
- S3: server line reports `from UNKNOWN port 65535` (`getpeername` gap).
