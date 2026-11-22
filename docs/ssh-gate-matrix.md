<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 19. Base `origin/master` `d50c35bb`; **K436 is not on master**
(it exists only at `origin/dave/K436` `9d1a30cb`). The master run used `ken/K434` at `5eb93bb0`
with a dedicated bridge/tap (`kenbr0`/`kentap0`); K436 was verified separately on the local scratch
branch `ken/K434-k436verify` (`b86cc544`, not pushed). K431 is still absent.

Run: `timeout 900 env RAVEN_QEMU_BRIDGE=kenbr0 RAVEN_QEMU_TAP=kentap0 sh tests/ssh/matrix.sh`
(exit 1; 4 red summary rows) and `timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Master matrix (d50c35bb)

| Step | Test | Expectation (source) | Result | Exact assertion / artifact |
|---|---|---|---|---|
| S0 | guest TCP accept/echo over tap | BSD `accept`/`read`/`write` (C1) | PASS | 30/30 bytes `K434-S0-ECHO-0123456789abcdef\n`, 297 ms |
| S1 | banner, fresh + control | OpenSSH `kex.c:1297`; `version.h:3` | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (1861 ms), control 1478 ms, client-first 1497 ms |
| S2 | KEXINIT | RFC 4251 section 6 | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20` |
| S3 | auth | OpenSSH `auth.c:264`, `auth-passwd.c:79` | PASS | wrong exit 255 + `Permission denied`; client `Authenticated to ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | post-auth read | POSIX `read`; FreeBSD `soreceive` | PASS | `hostkeys-00@openssh.com`, no `EFAULT` |
| S5 | confirmation delivery | POSIX `write`; FreeBSD `sosend` | PASS | `channel 0: open confirm rwindow 0 rmax 32768` |
| S5-identity | same file/sid | K431 fixture | BLOCKED | not on `origin/master` |
| S5-session | post-exec session | POSIX `read`/`write`; C10 | **FAIL** | no EPIPE; exit -9 at the 15 s cap; fd4 `socket:[269251562]`, `pipe:[269259340]`; TCP peer `ESTAB 10.0.2.2:55512 -> 10.0.2.15:22` |
| S6 | confirmation | RFC 4254 section 5.1 | PASS | `receive packet: type 91` + channel 0 |
| S6-length | 44-byte plaintext | `channels.c:2198`/`:5085` | BLOCKED | not in the client log |
| S7 | marker + 10/10 | gate C1-C10 | **FAIL** | client 0 bytes on the single attempt and 10/10 |
| S7-routing | client receives, console does not | C5/C9/C10 fd routing | **FAIL** | client 0 bytes; `max_console_new=0`; `console_total_delta=0` |
| S8-pty-alloc | dynamic pty case | K430/K436 libc test | PASS | `pty-alloc: pass` (17 s class) |
| S8 | PTY | POSIX `openpty`; OpenSSH `pty.c` | **FAIL** | `PTY allocation request failed on channel 0`; the guest also has no `tty` (`rsh: tty: command not found`) |

## K436 scratch verification (not on master)

`origin/dave/K436` = `9d1a30cb`; merged locally into `ken/K434-k436verify` = `b86cc544`. The static
`sshd-session` was rebuilt against the K436 libc (57 s) and the release image rebuilt (4m56).
Result:

- `pty-alloc` qtest: PASS.
- `ssh -tt raven@... tty`: **FAIL**, `PTY allocation request failed on channel 0`.
- S7: client 0 bytes; the delayed guest-console capture showed the exact marker 11 times, so the
  command ran and its stdout was not on the client channel.
- S5-session: no EPIPE; exit -9 at the 15 s cap; fd4 socket/pipe; TCP peer `ESTAB`.

K436 does not make S8 green in this tree. The remaining S8 failures are the PTY allocation request
and the absent `tty` utility; the S7 output path is separate.

## S7 fd-routing detail

The rev 19 master row asserts both ends:

- client stdout: 0 bytes on the single attempt and all ten 10/10 attempts;
- guest console marker count: `max_console_new=0`, `console_total_delta=0` in that run.

The K436 scratch run showed the opposite console side: 11 exact markers in the delayed console
capture. Neither run has the marker on the client channel. The row is red; the routing evidence is
recorded per run (master delta 0; K436 delayed total 11).

## S5-session detail

The previous rev 18 EPIPE was the test harness (`stdin=/dev/null`; `read(4)=0` mapped by
`sshbuf_read` to `EPIPE`). With a live stdin pipe there is no EPIPE. The client still does not
complete: `exit -9`, `timed_out=True`; fd4 `socket:[269251562]`, `pipe:[269259340]`; the TCP socket
fd 3 is `ESTAB 10.0.2.2:55512 -> 10.0.2.15:22`; the server-side handle is not exposed. S5-session
fails on completion, not on a fabricated socket errno.

## Artifacts (SHA-256)

```text
62ea7c9b8c698ced05c4da40190994d08c95069983e5d28a1aaf20100174b958  build/ssh/matrix/matrix.tsv
5c28923dcc6beae3138ea892ea2c9a9e7eba747eaaf5fbc0c2b05b82b53da616  build/ssh/matrix/attempts.tsv
5a44d948421318798852b28a85cd0cfc5ef7b959c18a9b562aef7e5c1237cd64  build/ssh/matrix/serial.log
7c3c1acb436695de78b3c9b1aa2d1b4ef8e9fdedb906711f970065f78c4c3410  build/ssh/matrix/serial-echo.log
96f5d40687dc506eaa3ef09166ebb146d77862b26dbe42612ff03be47c6da7d9  build/ssh/matrix/s7-marker.log
4cbcca582962953592e38a5f02afa41bd507e6db2f1c3d4ee356059bb0a8c326  build/ssh/matrix/s8-tt.log
6c7dac5049f925d37920908e41c28c144faf07146526b92ddc86cc2e3d6da4e9  build/ssh/matrix/pty-alloc.log
ec4e237b5cfa6bae3fdef0ecabdb839f506874555b1153d3cacf7dee353f6f13  build/ssh/matrix/s5-vvv.log-probe.log
9d185dd9b118e77e33dee988e92a5d17c4fecd2f2c95e98762e0421355379c43  build/ssh/matrix/s5-vvv.log-strace.log
51fae215ac760e4d5e93382c5a30d547147eb7462c46e3136d8ff9b3362d7c82  build/ssh/matrix/s8-server.log
ec3839e756500557b35dced1d5d6c5bcdbc771c32506c16a7b2222505bd6b405  master run log
bea7bca038cfb65c151a57c9687f6fa85ec223c579ae35693435b4c920dac9ab  K436 scratch run log
```

The K434 report carries the full acceptance record, gates and open gaps.

## Open gaps

- S5 identity needs K431's `socket-write-identity` fixture.
- S6 length is not observable from the client log.
- S3 peer address is `UNKNOWN` (`getpeername` gap).
- S7 stdout is not on the client channel; the master run also has console delta 0.
- S8 still fails PTY allocation; K436's scratch verification did not change it; `tty` is missing.
- K436 is not on `origin/master`; the scratch verification is not part of `ken/K434`.
