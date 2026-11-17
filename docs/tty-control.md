<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Controlling terminal and foreground process group

P2m closes the `process.tty-control`/`process.job-control` layer on top of the merged P3 tty
servers. The kernel owns the session/terminal state; `ttyd` remains the session server and data
handle, `consd` provides the console `tty:` scheme, and the kernel supplies spawn/setpgid/wait
path used by the fixture.

## Raw operations

The ABI contract crate is unchanged; the kernel-owned raw numbers are:

| Number | Operation | Arguments |
|---|---|---|
| `0x1e0` | `TIOCSCTTY` | `(tty_handle) -> 0` |
| `0x1e1` | `tcgetpgrp` | `(tty_handle, out: *mut i32) -> 0` |
| `0x1e2` | `tcsetpgrp` | `(tty_handle, pgrp) -> 0` |

`userland/rt/src/tty.rs` exposes `tiocsctty`, `tcgetpgrp` and `tcsetpgrp`. The handle must be a
scheme file whose server registered `ttyd` or `tty`; other handles return `ENOTTY`.

## Kernel state

`Process.ctty` points at a `Ctty` shared by the session. The registry key is
`(tty_server_pid, server_cookie)`; `ttyd` returns its session id as the cookie, so a child that
opens `ttyd:session/<id>` joins the same terminal. A `Ctty` records:

* the owning session id;
* the foreground process group;
* the hung flag.

`TIOCSCTTY` requires the caller to be a session leader with no controlling terminal and no other
session using that tty. It seeds the foreground group with the caller's process group.
`tcgetpgrp`/`tcsetpgrp` require the caller's session to match; `tcsetpgrp` validates that the
target group has a live member in the session, and unprivileged callers may only change it while
in the foreground (root and the session leader may change it).

## Background access

Before a scheme `READ`/`WRITE`/`PREAD`/`PWRITE` request reaches the server, the kernel checks the
caller against the tty's controlling state:

* foreground group: the request is forwarded normally;
* background, non-orphaned group: send `SIGTTIN` (read) or `SIGTTOU` (write) to the group and
  return `EINTR`;
* background, orphaned group: return `EIO` without a signal;
* hung terminal: return `EIO`.

The stop signals are the P2f job-control signals: the process gets the stop wait status and its
parent sees it through `PROC_WAIT_EX`. This slice keeps P2f's status-level stop/continue
model; it does not suspend the scheduler context.

## Hangup

When a session leader exits, `process::tty::hangup_session_leader` marks the terminal hung and
sends `SIGHUP` followed by `SIGCONT` to the foreground group. Subsequent accesses see `EIO`. The
fixture's grandchild proves the signal and the hung read. `ttyd`'s explicit `HANGUP` operation
marks its own session state; the kernel session-leader exit is the signalling path.

## ttyd and the fixture

`ttyd` now supports named, reference-counted sessions:

* `ttyd:session` allocates a session;
* `ttyd:session/<id>` joins an existing session;
* `SESSION_ID`, `SET_SID`/`GET_SID`, `SET_FG`/`GET_FG`, `HANGUP` and `STATE` use the existing
  12-byte CALL subset;
* the session handle has a synthetic read (`EAGAIN`) and write (`accepted`) path so the kernel's
  background check is observable without duplicating the pty data path.

The `tty-control` fixture starts `ttyd` before spawning (the child inherits the namespace
snapshot), acquires a controlling terminal, spawns a child into the same `ttyd` session, drives
foreground/background reads and writes, checks the orphaned `EIO`, and creates a session-leader
child with a foreground grandchild to exercise hangup.

## Limits

* The P2f stop is a wait status, not a scheduler suspension; the process continues after the
  interrupted syscall and `SIGCONT` only clears the status.
* The `ttyd` session handle is a control handle; pty data remains in `ptyd`. There is no second
  termios/data implementation.
* One controlling terminal per process/session; `TIOCSTI`, `TOSTOP` and background `tcsetpgrp`
  signal rules are not implemented.
* `consd` already registers the `tty` scheme and is accepted by the kernel check; the login/getty
  path still does not call `TIOCSCTTY`, so it keeps its existing behaviour.
* The executable P3 evidence overlay is outside this ticket's Owns; the coordinator refreshes the
  controlling-terminal row when merging P2m.
* No board claim.

## Tests

```
cargo xtask qtest --case tty-control --arch x86_64
cargo xtask qtest --case tty-control --arch aarch64
cargo xtask qtest --case proc-session --arch aarch64
cargo xtask qtest --case release-login --arch x86_64
```
