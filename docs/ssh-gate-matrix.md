<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate matrix (S0-S8)

Status: OBSERVED, 2026-11-22, rev 27. Base `origin/master` `c185d551`; merge `a0ae5f7ea`; code
`6f7e94334`. Boots are `-snapshot`; the main matrix runs in the private `ken434` netns. The gate is
RED.

Run: `timeout 900 sh tests/ssh/matrix.sh` (exit 1, 12:26) and `timeout 900 make check-fast`
(`check-fast: PASSED`).

## Final matrix (rev 27, master c185d551)

| Row | Result | Exact assertion |
|---|---|---|
| S0 | PASS | guest echoed 30 exact bytes (`K434-S0-ECHO-0123456789abcdef\n`) in 1156 ms |
| S1 | PASS | `SSH-2.0-OpenSSH_10.6\r\n` attempt 1 (11610 ms), control 6589 ms, client-first 4801 ms |
| S1-repeat | PASS | 10/10 banner + `Authenticated`; walls 8007-8046 ms; exits -9 |
| S2 | PASS | attempt 2 exact (`total=960 packet_length=956 padding=9 payload=946 type=20`); attempt 1 0 records |
| S3 | PASS | wrong password exit 255 + `Permission denied`; `Authenticated to 10.0.2.15 ...`; server `Accepted password for raven from UNKNOWN port 65535 ssh2` |
| S4 | PASS | attempt 1/5: `client_input_global_request: rtype hostkeys-00@openssh.com` and exact `K434-SSH-MARKER-2026-11-22\n`; no EFAULT |
| S5 | FAIL | no channel-open confirmation (exit 255) |
| S5-session | FAIL | no EPIPE; exit 255; `fd4=not captured`; TCP `10.0.2.2:54490 -> 10.0.2.15:22` ESTAB |
| S6 | FAIL | type 91 not observed (exit 255) |
| S7 | FAIL | 1/10 exact; 9 failures classified below; server/ps capture UNKNOWN |
| S7-routing | FAIL | client 0 bytes (not the marker) |
| S8 | BLOCKED | final-run S8 boot never reached `svcd: sshd up`; UEFI loader panic in `serial-s8.log` |
| S8 (dedicated boot) | FAIL | `PTY allocation request failed on channel 0`; full log `openpty: Broken pipe` |
| S8-pty-alloc | PASS | `pty-alloc: pass` |
| S8-termios | PASS | `termios: pass` |

The full matrix is not stable run to run: the previous full run (`/tmp/opencode/ken-k434-r27-matrix4.log`)
had S5/S6 PASS, S1/S2/S3/S4 PASS, S1-repeat 10/10, S7 0/10, and S8 reached the pty request. That
run's S8 full log and fixture output are the S8 evidence below. The final run's S8 boot panicked in
the UEFI loader (`Failed to write to stdout: Error`) and the bounded S8-only rerun
(`build/ssh/s8only/matrix.tsv`) timed out before the fixture. Both are recorded, not averaged.

## S4: per-attempt logging

`matrix.py:row_s4` runs up to five bounded `ssh -vv` attempts and records one `S4` row per attempt
with `auth`, `hostkeys`, exact bytes, exit and EFAULT. The final run passed on attempt 1/5 with:
`hostkeys=True bytes=K434-SSH-MARKER-2026-11-22\n exit=-9 efault=False`. The earlier run reached
all five attempts with `auth=False` and reported BLOCKED, with every attempt in `attempts.tsv`.

## S7: one row per failed attempt

Final run, `attempts.tsv` rows `S7-class`:

| Attempt | Bytes | Class | Exact witness / probe |
|---|---|---|---|
| 1 | 0 | banner-timeout | client `Connection timed out during banner exchange`; probe `SSH-2.0-OpenSSH_10.6\r\n` |
| 2 | 0 | connection-reset | client `kex_exchange_identification: Connection closed by remote host`; probe exact banner |
| 3 | exact | pass-bytes | exit -9 at 30099 ms (S7-exit failure) |
| 4 | 0 | connection-reset | client `Connection closed by remote host`; probe `Not allowed at this time\r\n` |
| 5 | 0 | guest-not-listening | client banner timeout; probe 0 bytes |
| 6 | 0 | connection-reset | client `Connection closed by remote host`; probe `Not allowed at this time\r\n` |
| 7 | 0 | connection-reset | client `Connection closed by remote host`; probe 0 bytes |
| 8 | 0 | connection-reset | client `Connection closed by remote host`; probe `Not allowed at this time\r\n` |
| 9 | 0 | banner-timeout | client banner timeout; probe exact banner |
| 10 | 0 | guest-not-listening | client banner timeout; probe 0 bytes |

The server log and `ps` capture after the storm both failed (`ssh exit -9`); they are UNKNOWN, not
empty. The full QEMU serial is `serial.log` (SHA-256 below). The exact class comes from the client
line and the immediate fresh banner probe; `Not allowed at this time` is the guest's own refusal
text, not a host-side classification.

## S8: exact call chain (no theory)

OpenSSH `addons/ssh/openssh/sshpty.c:57` `pty_allocate` calls
`openpty(ptyfd, ttyfd, NULL, NULL, NULL)` at `:63`; the failure is logged at `:65`
(`error("openpty: %.100s", strerror(errno))`). The monitor calls it from
`monitor.c:1694` (`res = pty_allocate(...)` in `mm_answer_pty`), and the network side reports
`session_pty_req: session 0 alloc failed` (`session.c:1890`).

Raven's C export rejects the call before any pty operation:

```text
libc/ravenlibc/crates/raven-libc/src/compat/openpty.rs:23
    if master.is_null() || slave.is_null() || name.is_null() {
        return -1;
```

It does not set errno. FreeBSD `lib/libutil/pty.c:47` `openpty` allows the name to be NULL
(`:74 if (name) strcpy(name, slavename);`); NetBSD `lib/libutil/pty.c:77` states `name may be NULL`.
The pty primitives themselves pass (K473's `pty-ssh-shape` case and this matrix's `pty-alloc` and
`termios` rows).

The S8 fixture (`tests/fixtures/ssh-openpty/openpty.c`, staged at `/bin/openpty-null`) calls the
exact OpenSSH shape and prints:

```text
openpty-null: call=null rc=-1 errno=1234 master=-1 slave=-1
openpty-null: call=name rc=0 errno=0 name=/dev/pts/1 master=3 slave=4
openpty-null: call=epipe-sentinel rc=-1 errno=32 strerror=Broken pipe
openpty-null: verdict=FAIL
```

`errno=1234` is a sentinel: the NULL-name call returns without touching errno. The EPIPE-sentinel
line shows `strerror(EPIPE)` is the exact string OpenSSH printed. The control with a name returns
`/dev/pts/1` with master=3, slave=4. Therefore **no pty write returned EPIPE**; `openpty: Broken
pipe` is a stale errno from the early NULL-name return. The minimal fix is outside K434's Owns:
allow `name == NULL` in `compat/openpty.rs` (pass `None` to `raven_libc_compat::openpty`) and only
write the name when non-NULL, matching FreeBSD `pty.c:74`.

The full sshd log from the dedicated S8 boot (matrix4) contains the exact sequence:

```text
Server listening on 0.0.0.0 port 22.
Accepted password for raven from UNKNOWN port 65535 ssh2
openpty: Broken pipe
session_pty_req: session 0 alloc failed
Connection closed by UNKNOWN port 65535
```

The guest has no `/proc` and no process-fd enumeration syscall; `ps -e` exposes only
`PID PPID UID GID CMD`. The final run's `ps`/log capture timed out (`exit -9`), so the privsep
monitor alive state and its fd table are **UNKNOWN**, not "not alive" and not empty. The full log
above is the captured server-side evidence.

## S1 A/B (rev 26, unchanged)

Same image, N=10: shared rev-24 `qemu-net` 5/10, private netns 4/10; both fail at banner/auth. The
network namespace is not the cause. Rev 27 did not rerun the A/B; the final run's S1-repeat passed
10/10 while S7 was 1/10, so the instability is load/connection-dependent.

## K443

`origin/tim/K443` = `d1a521093`; it does **not** contain `origin/master` `c185d551`. The conditional
S7-exit merge/run is not performed; `S7-exit` is not claimed.

## Artifacts (SHA-256)

```text
aad6ad4463a29a8638ac34530dae332cb4f6ac85ff4a5e3f02287ee5e40faac3  build/ssh/matrix/matrix.tsv
e4ca82ce07009e49ed0abdd6c6fccbe6a4d43c94f8057feb44e1087ea6ff5f10  build/ssh/matrix/attempts.tsv
009d7493cd63d069ec5d3cf886e21b174b7e3ceb04cbce1aa37cd7240a990ebf  build/ssh/matrix/serial.log
d44b8290f6a96797a43931bcd88c59d414fb93484cd2ab672201826cd584efe6  build/ssh/matrix/serial-repeat.log
00d38fe658ab9647e828ff7d63cafded1cae8701cce0ccbacab325eb9aec15b3  build/ssh/matrix/serial-s8.log
6d1f4c1178b1687dea4d685404596633c93c9902c8bb2f21c9e4b348e13675ce  build/ssh/matrix/serial-echo.log
58c3a2bfc148bca7a5a27954012cc0d75ade48107a94ccd843298eceb8da2f3f  build/ssh/matrix/s8-server.log
44ff5ec8465933e0a096bb18fd690cca800480b7458c52d5cdff899b31bfa060  build/ssh/matrix/s8-openpty-null.raw.log
b10aa7d952c40ea09ca9ae584d449d734c26f678053a2177b7891ed5eb27448b  build/ssh/matrix/s8-tt.log
03d2ab103215eeac5bdca0749e92822c49af1fc754f8395650a14e5371ea115e  build/ssh/matrix/s7-marker.log
1469878ae99c2fb5446eda3e2f56c7c92652ce717949352cde3f0667e2e45de6  build/ssh/matrix/pty-alloc.log
210a1c71c29dd571e5cb85cb462813d5c7cbe06382b3183d152dd7690d398101  build/ssh/matrix/termios.log
363347625f1847e9a05c5206131bc75181a6b71053a6ee81edc7a20461ac65e0  build/ssh/s8only/matrix.tsv
50fcd12a61623f4a68495ca604cdd5cbd2fb9cb92957be95344130693af2d2e9  build/ssh/s8only/serial-only.log
02d6a204a145dd9771dee9b37ba3ea7be785dc507276523956a904ed31ced0ae  final matrix log
e5a4ba60b5959bc5ae5c1b5714da0cf6e4aefd99031d4a594412b2b63a2c60c7  matrix4 log (S8 full log/fixture)
79b1371973ab926cc5de4fff3de4c7cdc169bc2ced595b4b7a6e9db323db441a  S8-only rerun log
```

## Open gaps

- S8: `openpty(..., NULL, ...)` returns -1 in `compat/openpty.rs:23`; the fix is a libc change outside
  K434 Owns. The monitor alive/fd table is UNKNOWN (no `/proc`, no fd enumeration, timed-out capture).
- S8 boot: the final run hit a UEFI loader panic; the S8-only rerun timed out before the fixture.
- S5-session: no EPIPE; exit 255/-9; the command session does not complete.
- S7: 1/10 exact; connection resets and `Not allowed at this time`; server log capture UNKNOWN.
- S7-exit: gated on `tim/K443` containing master; it does not.
- S1/S7 instability: 10/10 in one boot, 0/10 in another; load/connection dependent.
