<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 18. Base `origin/master` `d50c35bb` (K430 merged) merged into
`ken/K434` at `b5150cfc`; matrix code `7da596b1`. K431 is still absent. One throw-away S0 boot,
one `pty-alloc` qtest, and one release-image SSH boot on the K194 tap. The chain each row asserts is
in [`docs/ssh-gate-chain.md`](ssh-gate-chain.md); the lead's mapping is `tickets/ssh/SSH-CHAIN.md`.
**A row is pass or fail with exact bytes.** No class labels, no retries to green.

Run: `timeout 900 sh tests/ssh/matrix.sh` (exit 1; 3 red summary rows, 3 blocked sub-rows) and
`timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Step | Test | Expectation (source) | Result | Exact assertion / artifact |
|---|---|---|---|---|
| S0 | guest TCP accept/echo over the tap | BSD `accept`/`read`/`write` (C1) | PASS | 30/30 bytes `K434-S0-ECHO-0123456789abcdef\n`, 301 ms, `serial-echo.log` |
| S1 | sshd banner, fresh + control | OpenSSH `kex.c:1297` builds from `SSH_VERSION`; `version.h:3` is `OpenSSH_10.6` | PASS | attempt 1 `SSH-2.0-OpenSSH_10.6\r\n` (1879 ms); control 1546 ms; client-first probe 1541 ms |
| S2 | KEXINIT | RFC 4251 section 6 (C2/C3) | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20` |
| S3 | auth | OpenSSH `auth.c:264` `auth_log`, `auth-passwd.c:79` | PASS | wrong password exit 255 + `Permission denied`; correct client `Authenticated to ... using "password".`; serial server line `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | post-auth read | POSIX `read(2)`; FreeBSD `soreceive` (C2/C7) | PASS | `debug1: client_input_global_request: rtype hostkeys-00@openssh.com want_reply 0`; no `EFAULT`; the command output did not arrive (S5-session) |
| S5 | confirmation delivery | POSIX `write(2)`; FreeBSD `sosend` (C5/C6/C8) | PASS | `debug2: channel_input_open_confirmation: channel 0`; `channel 0: open confirm rwindow 0 rmax 32768` |
| S5-identity | exact N bytes on the same file/sid | K431 fixture | BLOCKED | `tests/fixtures/socket-write-identity` is not on `origin/master` |
| S5-session | post-exec session path | POSIX `read`/`write`; OpenSSH `serverloop.c`/`session.c` (C10) | **FAIL** | no EPIPE with live stdin; client killed at 15 s (`exit -9`, `timed_out=True`); sampled fd4 `socket:[268524506]`, `pipe:[268515101]`; TCP peer `ESTAB 10.0.2.2:49332 -> 10.0.2.15:22`; server-side handle not exposed |
| S6 | channel confirmation | RFC 4254 section 5.1; OpenSSH `channels.c` (C5) | PASS | `debug3: receive packet: type 91` + `channel 0: open confirm rwindow 0 rmax 32768` |
| S6-length | 44-byte plaintext packet | OpenSSH `channels.c:2198`/`:5085` | BLOCKED | client log exposes type and recipient channel, not the packet length |
| S7 | marker + 10/10 | gate (C1-C10) | **FAIL** | client stdout 0 bytes on the single marker attempt and 10/10; the guest serial log shows the exact marker **11 times**, so the command ran and its stdout went to the guest console, not the SSH channel |
| S8 | PTY | POSIX `openpty`; OpenSSH `pty.c` (C9) | **FAIL** | `PTY allocation request failed on channel 0`; the guest console also reports `rsh: tty: command not found`; the pty-alloc qtest PASSes |
| S8-pty-alloc | dynamic pty case | K430 `tests/libc/c/libc-pty-alloc.c` | PASS | `timeout 45 cargo xtask qtest --case pty-alloc --arch x86_64` exit 0, `pty-alloc: pass` (17.3 s measured) |

The per-attempt table is `build/ssh/matrix/attempts.tsv`: attempt number, fresh/reused connection,
expected and received bytes, elapsed milliseconds, result and detail.

## Rev 18 corrections (facts, not labels)

1. **The old S5-session EPIPE was the test harness.** The previous run used
   `stdin=/dev/null`; strace showed `read(4</dev/null<char 1:3>>, "", 32768) = 0`.
   `sshbuf_read` (`addons/ssh/openssh/sshbuf-misc.c:340-353`) maps a zero-length read to
   `SSH_ERR_SYSTEM_ERROR` with `errno = EPIPE`, so OpenSSH logged `Broken pipe`. It was not a
   network-socket EPIPE. With a live stdin pipe the row has no EPIPE; the client still does not
   complete and is killed at the 15 s cap. The row now fails on completion, and records the sampled
   fd4 socket/pipe and the `ESTAB` TCP peer.
2. **S7 command execution is real; the output path is not.** The guest serial log contains
   `K434-SSH-MARKER-2026-11-22` 11 times (one single attempt + ten 10/10 attempts), while every SSH
   client received 0 bytes. The session stdout is on the guest console, not the SSH channel.
3. **S8 pty-alloc passes but `ssh -tt` fails.** K430's dynamic qtest passes as root and uid 1000.
   The SSH PTY request still answers `PTY allocation request failed on channel 0`; the guest also
   lacks a `tty` executable (`rsh: tty: command not found`), so the exact `/dev/pts/N` assertion
   cannot pass even if the allocation is fixed. The rebuilt static `sshd-session` (51 s build) did
   not change this.
4. **S3 server line.** The exact line is `Accepted password for raven from UNKNOWN port 65535 ssh2`;
   `getpeername`/`ssh_remote_ipaddr` does not return `10.0.2.2`. The auth line itself is present.
5. **S1 banner.** The wire bytes are `SSH-2.0-OpenSSH_10.6\r\n`: `version.h:3` is
   `SSH_VERSION "OpenSSH_10.6"`, `:6` is `SSH_RELEASE SSH_VERSION SSH_PORTABLE`, and `kex.c:1297`
   uses `SSH_VERSION`. The ticket's `10.6p2` is the command-line release, not the banner.

## Artifacts (SHA-256)

```text
matrix.tsv, attempts.tsv, serial.log, serial-echo.log, s3-server.log,
s7-marker.log, s7-run-1..10.log, s8-tt.log, s8-server.log, pty-alloc.log,
s5-vvv.log, s5-vvv.log-probe.log, s5-vvv.log-strace.log
```

The exact hashes are in the K434 report, which is the acceptance record. The large artifacts stay in
`build/ssh/matrix/`; they are reproducible with `sh tests/ssh/matrix.sh`.

## Test-first and negative evidence

- The rows were written as exact assertions before this run. The failed values are preserved in
  earlier runs: the first S1 probe was 0 bytes on the rev-16 harness, S2 produced `total_read=0` in
  one run, S3's wrong-password attempt timed out during banner exchange in one run, and S5-session
  produced the `/dev/null` read/EPIPE shape before the stdin fix.
- The wrong-password row is the negative auth check: exit 255 + `Permission denied`; the correct
  row must show `Authenticated to` and the server `Accepted` line.
- S2 rejects a 0-byte/0-record probe and accepts only the exact framed KEXINIT.
- S8's pty-alloc qtest passes on the same base while `ssh -tt` fails, so the two assertions are
  independent and neither is vacuous.

## Open gaps

- S5's same-file/sid identity assertion needs K431's `socket-write-identity` fixture (not on this
  base). S5 records client-side delivery only.
- S6's 44-byte plaintext length is not in the client debug log; the type-91/recipient-channel parse
  is. A server-side instrumented build would be needed for the length.
- S3's server line reports `from UNKNOWN port 65535`; the peer address is not observable from
  `getpeername`.
- S7's stdout is on the guest console; the server-side session fd mapping to the SSH channel is the
  open defect.
- S8's `ssh -tt` allocation still fails after K430 and the static `sshd-session` rebuild; the guest
  `tty` utility is also missing. Both are separate fixes.
