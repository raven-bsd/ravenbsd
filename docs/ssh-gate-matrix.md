<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, final run on `ken/K434` at `0c4d425c`, base
`origin/master` `b8472426` (K427 + K429; **K430 and K431 are not merged**). One throw-away S0
boot plus one release-image SSH boot on the K194 tap. The chain each row asserts is in
[`docs/ssh-gate-chain.md`](ssh-gate-chain.md); the lead's mapping is `tickets/ssh/SSH-CHAIN.md`.
**A row is pass or fail with exact bytes.** No class labels, no retries to green.

Run: `timeout 900 sh tests/ssh/matrix.sh` (exit 1; 3 red summary rows, 4 blocked sub-rows) and
`timeout 900 make check-fast` (exit 0; `check-fast: PASSED`).

## Matrix

| Step | Test | Expectation (source) | Result | Exact assertion / artifact |
|---|---|---|---|---|
| S0 | guest TCP accept/echo over the tap | BSD `accept`/`read`/`write` (C1) | PASS | 30/30 bytes: `K434-S0-ECHO-0123456789abcdef\n`, 344 ms, `serial-echo.log` |
| S1 | sshd banner, fresh connection + control | OpenSSH `kex.c:1297` builds the wire banner from `SSH_VERSION`; `version.h:3` is `OpenSSH_10.6` | PASS | attempt 1 `SSH-2.0-OpenSSH_10.6\r\n` (2438 ms); control attempt 2 the same (1898 ms); a client-first probe the same (1702 ms) |
| S2 | KEXINIT | RFC 4251 section 6 (C2/C3) | PASS | `total=960 packet_length=956 padding=9 payload=946 type=20 (SSH_MSG_KEXINIT)` |
| S3 | auth | OpenSSH `auth.c:264` `auth_log`, `auth-passwd.c:79` | PASS | client `Authenticated to 10.0.2.15 ([10.0.2.15]:22) using "password".`; serial server line `Accepted password for raven from UNKNOWN port 65535 ssh2`; wrong password exit 255 + `Permission denied` |
| S4 | post-auth read | POSIX `read(2)`; FreeBSD `soreceive` (C2/C7) | PASS | `debug1: client_input_global_request: rtype hostkeys-00@openssh.com want_reply 0`; no `EFAULT`. Command output did not arrive (exit 124); that is the post-exec path below |
| S5 | post-auth confirmation delivery | POSIX `write(2)`; FreeBSD `sosend` (C5/C6/C8) | PASS | `debug2: channel_input_open_confirmation: channel 0` and `channel 0: open confirm rwindow 0 rmax 32768`; file/sid identity is BLOCKED (K431) |
| S5-identity | exact N bytes on the same file/sid | K431 fixture | BLOCKED | `tests/fixtures/socket-write-identity` is not on `origin/master` |
| S5-session | post-exec session path | OpenSSH `serverloop.c` / `session.c` (C10) | **FAIL** | `debug2: channel 0: read failed rfd 4 maxlen 32768: Broken pipe` after `exec request accepted`; exit 124 |
| S6 | channel confirmation | RFC 4254 section 5.1; OpenSSH `channels.c` (C5) | PASS | `debug3: receive packet: type 91` + `channel 0: open confirm rwindow 0 rmax 32768` (channel 0) |
| S6-length | 44-byte plaintext packet | OpenSSH `channels.c:2198`/`:5085` | BLOCKED | the client log exposes type and recipient channel, not the packet length |
| S7 | `ssh raven@... cat /tmp/ssh-marker` then 10/10 | gate (C1-C10) | **FAIL** | marker: 0 bytes (exit 124); 0/10 command sessions (8 exit 124, 2 exit 255) |
| S8 | PTY | POSIX `openpty`; OpenSSH `pty.c` (C9) | **FAIL** | `PTY allocation request failed on channel 0`; K430 unmerged |
| S8-pty-alloc | dynamic pty test | K430 `tests/libc/c/libc-pty-alloc.c` | BLOCKED | the case is not on `origin/master` |

The per-attempt table is `build/ssh/matrix/attempts.tsv`: every row has the attempt number, fresh or
reused connection, expected and received bytes, elapsed milliseconds, result and detail.

## Notable exact observations

1. **Banner (S1).** The ticket's `SSH-2.0-OpenSSH_10.6p2` expectation contradicts the cited source:
   `version.h:3` is `SSH_VERSION "OpenSSH_10.6"`, `:6` is `SSH_RELEASE SSH_VERSION SSH_PORTABLE`, and
   `kex.c:1297` builds the identification string with `SSH_VERSION`. The wire bytes are exactly
   `SSH-2.0-OpenSSH_10.6\r\n` (22 bytes). S1 asserts the source bytes.
2. **Auth server line (S3).** The exact line is `Accepted password for raven from UNKNOWN port 65535 ssh2`.
   The peer address is `UNKNOWN`, not `10.0.2.2`: `ssh_remote_ipaddr`/`getpeername` does not return the
   peer for this socket. The auth line itself is present and correct; the peer-address value is an
   open observation, not hidden by the row.
3. **Post-auth write (S5) vs post-exec (S5-session).** The 44-byte confirmation is delivered (S5/S6
   PASS), then the client reports a read failure on the channel after the exec request. That is the
   command/session path, a separate chain step; S5 is not relabelled.
4. **First-connection stalls are characterised, not labelled.** S2 failed with `total_read=0` in an
   earlier run and passed in the final run; S3's first sample timed out in an earlier run and passed
   in the final run. The per-attempt table keeps the failed samples and their timings.
5. **S7 and S8 are the gate blockers.** S7 has no command output at all (0/10); S8 is the unmerged
   K430 PTY path. The ticket's green condition is not met.

## Artifacts (SHA-256)

```text
6c6a8bdc065f5073426b2609ee6ad209f6b9fd68b5a76b8e61fbb98d2a6ccdd6  build/ssh/matrix/matrix.tsv
171382e16622ced7ed35377ebeb2cbe394043880caf1bb6df13ee77983635242  build/ssh/matrix/attempts.tsv
23f974a3c04eb33997cc0da5db241e94bb84115bf704a04029f7d47e1d72fa71  build/ssh/matrix/serial.log
72e71bf751b2ef95de72b013df7c21336c8063f7e85d9ccb2b06a5321f804dd1  build/ssh/matrix/serial-echo.log
386c648089d7b0a492fc8dca8391daf8e5fb6f38dee8ff88be5573bd05fee3e3  build/ssh/matrix/s3-raven.log
8131fa6220b39f36490e38f3a22f4fefd104a5f61e88e00ba5ae3066137f897e  build/ssh/matrix/s4-vv.log
371d8f05ceb192f0099d9d3e2f1f521ae64567b051c0a1255653dc2b98db596f  build/ssh/matrix/s5-vvv.log
614a07819bad69cbe20cdc6af2e42df8e63a645ee4042e4481be46bb00d652ad  build/ssh/matrix/s8-tt.log
001fa5e7d3073d5e1904f48acdb00027e3bf9bf381170fec22970b1cc206e0be  matrix run log
9303766c6765159890a14defa08a266516dfd359e6a1577d6cc56979d529b82a  make check-fast log
478cd5b0a26c75049eaacbc412160f2c09cf3c6d4cc06f9b12c28ee69f0bee09  provenance check log
```

## Test-first and negative evidence

- The rows were written as exact assertions before this run. Earlier runs on this branch produced the
  negative values used here: S1's first probe was `0 bytes` on the rev-16 harness (the readiness
  pre-connect was removed), S2 produced `total_read=0` in one run, S3's first sample timed out, and
  S5-session produced the exact `Broken pipe` line while S5/S6 still passed. Those are the failing
  cases the final assertions separate.
- The wrong-password row is the negative auth check: it must exit 255 with `Permission denied`, while
  the correct row must show `Authenticated to` and the server `Accepted` line. An accept-all auth path
  fails S3.
- The S2 parser rejects a 0-byte/0-record probe (observed) and accepts the exact framed KEXINIT, so
  the assertion is not vacuous.
- S8's failure is produced by the same `/dev/pts/N` assertion that must pass; no timeout or retry is
  used to hide it.

## Open gaps

- S5's same-file/sid identity assertion needs K431's `socket-write-identity` fixture (not on this
  base). S5 records the client-side delivery fact only.
- S6's 44-byte plaintext length is not in the client debug log; the type-91/recipient-channel parse is.
  A server-side instrumented build would be needed for the length; that build is not on this base.
- S3's server line reports `from UNKNOWN port 65535`; the exact peer address is not observable from
  `getpeername` in this image (open observation).
- S8's `pty-alloc` dynamic case and `/dev/pts/N` both depend on K430, which is not merged to
  `origin/master`; the ticket's nominal base `d50c35bb` does not exist in this clone (master is
  `b8472426`).
- S7's command path fails before output; the exact client line is `channel 0: read failed rfd 4
  maxlen 32768: Broken pipe` after `exec request accepted`. That is the gate blocker for 10/10 and
  for the marker.
