<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# procd process-model server (P2e)

OBSERVED on `stuart/P2e` (base `origin/master` `14908851`, P2f merged). The server, protocol,
kernel wait/group/signal seam and Qtests are exercised. No board claim.

## Verdict

`procd` registers `proc:` and accepts version-1 `CALL` requests on an opened connection. It keeps
a table only of children **it created**; the kernel remains authoritative for identity, address
spaces, wait statuses and signal queues. The P2f kernel primitives (`PROC_WAIT_EX` 0x48,
`PROC_GETPGID` 0x49, `PROC_SIGNAL` 0x4A) are used directly. The server never invents a process and
answers `ENOSYS` for an operation whose kernel/client seam is absent.

## Transport and wire format

* One connection is an `OPEN proc:` file; `CALL` carries request and response. A call on an
  unknown connection id is `EBADF`.
* All integers are little-endian; four-byte fields align to four and eight-byte fields to eight;
  strings are `u32` length plus bytes and padding; argument vectors are `u32 count`, `u32 total
  bytes`, NUL-terminated entries and padding.
* `RequestHeader { version: u16 = 1, op: u16, flags: u32, seq: u64 }` and
  `ResponseHeader { status: i32 = -errno, reserved: u32, value: i64 }` are each 16 bytes.
* `raven-proto-proc` owns the codec. Unknown op numbers are `EINVAL`; a wrong version is
  `EPROTO`; `status` is `0` or `-errno`.

## Implemented operations

| Op | State | Behaviour |
|---|---|---|
| `HELLO` | works | Server version 1 and the `procd` pid. |
| `SPAWN` | works, bounded | Initfs image with UTF-8 argv/env. `POSIX_SPAWN_SETPGROUP` is applied through `PROC_SETPGID`; `pgroup=0` means the child pid. Fast-exit children fall back to the server's group/session for the record. Signal masks, scheduler fields, `tcpgrp_fd` and fdmap are `ENOSYS`; relative path, empty argv, NUL or invalid UTF-8 are `EINVAL`. `umask` is recorded but the kernel has no child-umask call. |
| `WAIT` | works | `P_PID`/`P_PGID`/`P_ALL`, `WNOHANG`/`WUNTRACED`/wire `WCONTINUED` (translated to P2f's kernel value), blocking and status encoding from `PROC_WAIT_EX`. `siginfo`/`rusage` are zero (the kernel exposes status only). Ownership is checked against the connection uid. |
| `KILL` | works for owned children | Any kernel-valid signal through `PROC_SIGNAL`; `SIGKILL` terminates (137), stop/continue set wait events, other signals queue for `PROC_SIGRECV`. Signal 0 is an existence check. A negative pid addresses a child group. `kill(0)`/`kill(-1)` and `sigqueue` are `ENOSYS` because the server has no caller pid. |
| `SETPGID` | works | A direct child owned by the connection; `pgid=0` means the child pid. `pid=0` is `ENOSYS` (no client pid). |
| `GETPGID` | works | Explicit positive pid through `PROC_GETPGID`; `pid=0` is `ENOSYS`. |
| `GETSID` | works | Explicit positive pid through `PROC_PID_INFO`; `pid=0` is `ENOSYS`. |
| all other ops | `ENOSYS` | `FORK`, `EXEC`, `SETSID`, `SETCRED`, `GETGROUPS`, `RLIMIT`, `RUSAGE`, `PRIO`, `SCHED`, `SETTITLE`, `PRCTL`, `LOGIN`, `PLEDGE`, `CAPENTER`, `SESSIONINFO`. The codec recognises them so a client gets a clean error, not a protocol mismatch. |

## Process matrix

`docs/posix/evidence/P2.toml` closes the P2 rows. The executable rows run the kernel Qtests; the
remaining rows have an explicit owner waiver, not a fake pass.

| Row | State | Evidence / waiver |
|---|---|---|
| `process.credential` | present | `qtest:proc-credentials` (P2c uid/gid/saved/group rules). |
| `process.fork` | present | `qtest:proc-fork` (COW fork, private/NOFORK pages, repeated cycles). |
| `process.process-id` | present | `qtest:proc-registry` (pid registry and `PROC_PID_INFO`/`PROC_HANDLE_INFO`). |
| `process.process-group` | present | `qtest:proc-session` (setpgid/getpgid and group signalling). |
| `process.session` | present | `qtest:proc-session` (setsid/getsid and membership). |
| `process.signal` | present | `qtest:proc-signal` (queue, SIGCHLD, SIGKILL/stop/continue). |
| `process.terminate` | present | `qtest:proc-session` (kill statuses). |
| `process.wait` | present | `qtest:proc-session` (`P_PID`/`P_PGID`/`P_ALL`, options and status encoding). |
| `process.times` | present | `qtest:proc-session` (times/getrusage; user time and extra counters remain zero). |
| `process.resource` | present | `qtest:proc-session` (16 inherited limits; `RLIMIT_NPROC` enforced). |
| `process.exec` | waived | Same-pid replacement needs `PROC_RESET` (not in P2f); only spawn of a new image exists. |
| `process.job-control` | waived | P2f reports stop/continue but does not suspend threads; P3 owns scheduler/terminal glue. |
| `process.tty-control` | waived | P3 owns ttyd controlling terminal and `SESSIONINFO`. |
| `process.popen` / `process.system` | waived | No pipe primitive or libc procd client (P3/P17). |
| `process.priority` | waived | No scheduler priority syscall. |
| `process.posix-spawn` | waived | Native libc spawn exists but its QEMU case needs a built target sysroot; procd is not wired into libc (P17). |

## Evidence

```
$ cargo nextest run -p raven-proto-proc
7 tests, 7 passed, exit 0

$ cargo nextest run -p raven-procd
17 tests, 17 passed, exit 0

$ cargo xtask qtest --case proc-fork --arch x86_64
proc-fork: pass

$ cargo xtask qtest --case proc-exec --arch x86_64
proc-exec: pass

$ cargo xtask qtest --case proc-procd-signal --arch x86_64
proc-procd-signal: pass

$ cargo xtask qtest --case proc-signal --arch x86_64
proc-signal: pass

$ cargo xtask qtest --case proc-session --arch x86_64
proc-session: pass
```

`proc-procd-signal` serial evidence:

```
procd-test: hello version=1 pid=3
procd-child: sleeping
procd-test: spawn pid=4
procd-test: setpgid pid=4
procd-test: getpgid=4
procd-child: got signal 15
process: child exited (143)
procd-test: kill signal=15
procd-test: wait exit=143
procd-test: unknown=echild
procd-test: unsupported=enosys
procd-test: ok
init exited (0)
```

`posix-check` now reports all 17 process rows as pass (17 pass, 0 fail). The overall gate still
reports `FAIL (release blocked)` because other owner areas are incomplete; that is not a process
row result and is not claimed as a pass.

## Limits and follow-ups

* **No client pid.** `Ctx` carries credentials only. The server enforces ownership by connection
  uid and cannot implement caller-relative operations (`pid=0`); a client-id stamp or a transferred
  process handle is needed.
* **No same-pid exec.** `EXEC` needs `PROC_RESET`; `SPAWN` is the supported creation path.
* **No server-side credential/rlimit proxy.** The kernel qtests close those rows, but the `SETCRED`
  and `RLIMIT` protocol operations cannot target a client/child without a process-handle seam.
* **Blocking wait.** A `WAIT` call blocks the single server loop. A deferred waiter is a follow-up.
* **No fd transfer.** `SPAWN` rejects an fdmap; the child is initfs-only.
* **No boot integration.** Profiles/service manifests are outside P2e's owns; the fixture starts
  `procd` from its initfs. A profile service entry is a later change.
* **P3.** Scheduler stop/resume, controlling terminals, termios and pty job control remain P3.
* No board claim.
