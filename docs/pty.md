<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# PTY and the privsep exchange

RavenBSD's pty support spans the `ptyd` scheme, ravenlibc's pty calls, and the OpenSSH
privsep exchange. The SSH server is the unmodified upstream OpenSSH 10.6p2 add-on
(`addons/ssh/openssh/`); no SSH code lives in the base tree.

## Layers

| Layer | Location | Contract |
|---|---|---|
| pty scheme | `userland/servers/ptyd/` | One line discipline and two byte queues per pair; `pty:master[/N]` and `pty:slave[/N]`; `pty:ptmx` allocates the lowest free pair, whose last close releases it; `SET_MODE`/`GET_MODE`/`FLUSH`, the complete `GET_TERMIOS`/`SET_TERMIOS` record, `GET_WINSIZE`/`SET_WINSIZE`, foreground group and hangup |
| libc pty calls | `libc/ravenlibc/crates/raven-libc/src/compat/` | `posix_openpt`, `grantpt`, `unlockpt`, `ptsname`, `ptsname_r`, `openpty` |
| libc termios | `libc/ravenlibc/crates/raven-libc-platform/src/ioctl/tty.rs` | `tcgetattr`/`tcsetattr` over the full record; older console servers fall back to `GET_MODE`/`SET_MODE` |
| libc winsize | `libc/ravenlibc/crates/raven-libc-platform/src/ioctl/tty.rs` | `tcgetwinsize`/`tcsetwinsize` and the `TIOCGWINSZ`/`TIOCSWINSZ` ioctl path; non-tty -> `ENOTTY` |
| identity | `ptyd` `fstat` | Pair `N` master inode `2N+1`, slave inode `2N+2`; both non-zero, role-distinct |
| privsep exchange | OpenSSH `monitor_wrap.c`/`monitor.c`; test `tests/libc/c/libc-pty-monitor.c` | Monitor allocates the pty and returns the master/slave descriptors plus the name; the session opens the slave by name |

## Termios round-trip (K444)

OpenSSH's pty-req path applies the client's RFC 4254 section 8 mode word on the pty slave
(`session.c:1888 session_pty_req` -> `ttymodes.c:342 ssh_tty_parse_modes` -> `tcgetattr`,
parse, `tcsetattr(TCSANOW)` at `ttymodes.c:447`). The pty therefore carries the complete
`struct termios`, not just canonical/raw and echo:

| Wire offset | Field |
|---|---|
| 0 | version (`u16`, 1) |
| 2 | op (`u16`: `GET_TERMIOS` 6, `SET_TERMIOS` 7) |
| 4..20 | `c_iflag`, `c_oflag`, `c_cflag`, `c_lflag` (`u32` little-endian) |
| 20 | `c_line` (`u8`) |
| 21..53 | `c_cc[32]` |
| 53..61 | ispeed, ospeed (`u32` little-endian) |

A body is 69 bytes (`TERMIOS_LEN`). No struct memory is sent, so the ABI padding bytes never
travel. `ptyd` keeps one record per pair, so the master and slave read the same attributes;
`SET_TERMIOS` changes attributes only and never touches the queued output, which makes
`TCSADRAIN` lossless by construction. The line discipline uses the NetBSD `ttyinput_wlock` /
`ttyoutput` rules for `ICRNL`/`INLCR`/`IGNCR`/`ISTRIP`/`IXON`/`ISIG`/`ICANON`/`ECHO` and
`OPOST`/`ONLCR`/`OCRNL`; bits with no hardware behind them are preserved exactly.

The guest case sets the exact SSH-style record (including `VMIN`/`VTIME`, `c_cc` entries and
distinct ispeed/ospeed), reads it back on the slave and master, and then queues output and
applies a second `TCSADRAIN` record:

```sh
timeout 600 cargo xtask qtest --case termios --arch x86_64
# termios: pass
```

Evidence: `tests/evidence/k444/<sha>/` contains the pass log and JSON, the pre-fix red unit
log, the field-mutation qtest failure and `SHA256SUMS`.

## Window size (K455)

OpenSSH sends the client's geometry in the pty request (`session.c` `pty_set_size` /
`TIOCSWINSZ`). The pty stores one `struct winsize` per pair and returns it unchanged on both
ends; a fresh pair starts at `0x0`, matching NetBSD. The four fields travel in a 20-byte body:

| Wire offset | Field |
|---|---|
| 0 | version (`u16`, 1) |
| 2 | op (`u16`: `GET_WINSIZE` 8, `SET_WINSIZE` 9) |
| 4..8 | `arg0` (zero) |
| 8..12 | `arg1` (zero) |
| 12..14 | `ws_row` (`u16` little-endian) |
| 14..16 | `ws_col` (`u16`) |
| 16..18 | `ws_xpixel` (`u16`) |
| 18..20 | `ws_ypixel` (`u16`) |

`tcgetwinsize`/`tcsetwinsize` and `ioctl(TIOCGWINSZ/TIOCSWINSZ)` use the same record; a
non-tty fd answers `ENOTTY`, never `ENOSYS`. The guest case sets 43x120x0x0 on the slave,
reads it on slave/master and through the ioctl path, sets 24x80x640x480 from the master, sets
zeros, and checks the non-tty errno:

```sh
timeout 600 cargo xtask qtest --case pty-winsize --arch x86_64
# pty-winsize: pass
```

Evidence: `tests/evidence/k455/<sha>/` contains the pre-fix `ENOSYS` serial, the pass log and
JSON, the `ws_col` mutation failure and `SHA256SUMS`.

## Lifetime and churn (K458)

`posix_openpt` opens the clone device `pty:ptmx`; the pty server allocates the lowest free pair
id, the same unit-pool rule as NetBSD `ptmopen`/`pty_alloc_master` and FreeBSD `pts_alloc`/
`ptsdrv_free`. The caller reads the pair id back from the endpoint inode (`2N+1` master), so
`ptsname` names `/dev/pts/<N>` without a process-local counter. Two concurrent processes never
share a pair. When the last scheme file of a pair closes, the server removes the pair; a test
hook prints `ptyd: files=0 pairs=0` at startup and on every transition to an empty table.

The `pty-churn` case runs four fork workers through 50 cycles each, holds 32 pairs at once,
checks the slave paths are distinct, and proves descriptor reuse and a zero handle count at
worker exit. The kernel's `s24-framecheck` records the free-frame count after each worker exit:

```sh
timeout 600 cargo xtask qtest --case pty-churn --arch x86_64
# pty-churn: pass cycles=200 concurrent=32
```

The four worker exits print increasing free-frame counts because each exit releases that
worker while the other three are still alive; the count never falls, and the final post-churn
value is the baseline. Evidence: `tests/evidence/k458/<sha>/`.

## SSH-shape integration (K473)

`pty-ssh-shape` runs the exact server-side pty sequence without sshd or the network: the libutil
`openpty` call order (`posix_openpt`, `grantpt`, `unlockpt`, `ptsname`, `open` slave), the K444
pty-req termios record, the K455 43x120 window size, a fork+`execve` session child on the slave
that prints `ttyname(0)` and `tcgetwinsize(0)`, an ECHO/input round-trip, then close-all. Each call
logs `pty-ssh-shape: step=N call=... rv=...`, so a failure names the syscall.

```sh
timeout 300 cargo xtask qtest --case pty-ssh-shape --arch x86_64
# pty-ssh-shape: pass
```

The child is a true `fork` + `execve` of the test binary. `posix_spawn` of the same dynamic
`/bin/libc-test` returned `ENOENT` (2) in the libc-test profile; `execve` works and is the path
OpenSSH uses. The case is the reference the S8 `openpty: Broken pipe` owner should cite.

## Line discipline behaviour (K485)

The stored `Termios` record drives the pty, not just the round-trip:

* canonical `VERASE`/`VKILL`/`VEOF` and `VWERASE` edit the line; echo is gated by `ECHO`;
* `ISIG` consumes `VINTR` (`^C`) and the discipline raises the interrupt event; foreground pgrp
  delivery still needs a `tcsetpgrp` path, so that part is not claimed;
* `OPOST|ONLCR` maps slave `\n` to `\r\n` on the master;
* `ICRNL`/`INLCR`/`IGNCR`/`ISTRIP` apply to input in both canonical and raw mode (raw skips only
  editing/echo; `cfmakeraw` clears the translation flags for a true byte pipe);
* `VMIN`/`VTIME` are applied in the `ptyd` slave read with a monotonic deadline and 1 ms yield.
  A queued byte with `VMIN=1` returns immediately; `VMIN=0,VTIME=1` returns 0 after ~100 ms.
  The server is single-threaded, so a read that waits for a *later* concurrent master write
  blocks the server; that wait-queue case is a known gap (it needs a pty event/wait design), not
  claimed by the case.

The guest case `tty-discipline` drives each row through `tcsetattr`/`read`/`write` and prints the
observed bytes/timing:

```sh
timeout 300 cargo xtask qtest --case tty-discipline --arch x86_64
# tty-discipline: pass
```

## Privsep wire shape

The `pty-monitor` guest test implements the monitor side over `socketpair(AF_UNIX)`:

1. Session: 4-byte big-endian length `1`, type `MONITOR_REQ_PTY` (28).
2. Monitor: `posix_openpt`/`grantpt`/`unlockpt`/`ptsname`, then `open` the slave.
3. Monitor: 4-byte length, type `MONITOR_ANS_PTY` (29), payload
   `success` (`u32`), `name` (cstring), login message (cstring), then the test-only
   master/slave `dev`/`ino` identity fields.
4. Monitor: two `monitor_fdpass`-style messages (one data byte plus one `SCM_RIGHTS` cmsg):
   the master, then the slave.
5. Session: verifies `cmsg_len`, `SOL_SOCKET`, `SCM_RIGHTS`, the count and `msg_controllen`,
   opens the slave by name, checks `fstat` inode identity, and round-trips a byte each way.
6. Session: `MONITOR_REQ_PTYCLEANUP` (30); the monitor closes the pair and exits.

Run it with:

```sh
timeout 300 cargo xtask qtest --case pty-monitor --arch x86_64
```

The test also includes three negative exchanges: the monitor drops the master fd, the reply
names a different pty (the opened slave's inode does not match the passed slave), and the
request omits its type byte.

## External SSH gate

`ssh -tt raven@<guest> tty` must print `/dev/pts/<N>`. The host gate is
`tests/qtest/cases/ssh-login.toml` (K195) with the release image built from
`cargo xtask image --arch x86_64 --profile release --output build/qtest/ssh-login.img`.
The guest-side `pty-monitor` test isolates the exchange itself; the host gate exercises the
same path through the real OpenSSH monitor. K444 settles the termios half of the pty-req
contract; the host gate remains blocked at the K195 session-delivery stage (the OpenSSH add-on
binaries are not staged in this clone, and the documented 0-byte TX/session failure predates
this ticket), so no `ssh -tt` result is claimed here.
