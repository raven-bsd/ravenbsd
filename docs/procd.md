<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# procd process-model server (P2e, P2n spawn attributes)

OBSERVED on `stuart/P2n` (base `origin/master` `1e42f192`; P2e, P2f, P2k and P2m merged). The
server, protocol, kernel wait/group/signal seam and Qtests are exercised. No board claim.

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
| `SPAWN` | works, bounded | Initfs image with UTF-8 argv/env. Accepts the P2n fd map, `POSIX_SPAWN_SETPGROUP`, `POSIX_SPAWN_SETSIGMASK`/`SETSIGDEF`, and the `S_USE_PATH_HINT`/`S_CLOEXEC_DEFAULT` hints; see "Spawn attributes" below. `pgroup=0` means the child pid. Fast-exit children fall back to the server's group/session for the record. `SETSID`, scheduler fields, `tcpgrp_fd` and a non-root relative `cwd` are `ENOSYS`; empty argv, NUL or invalid UTF-8 are `EINVAL`. `umask` is recorded but the kernel has no child-umask call. |
| `WAIT` | works | `P_PID`/`P_PGID`/`P_ALL`, `WNOHANG`/`WUNTRACED`/wire `WCONTINUED` (translated to P2f's kernel value), blocking and status encoding from `PROC_WAIT_EX`. `siginfo`/`rusage` are zero (the kernel exposes status only). Ownership is checked against the connection uid. |
| `KILL` | works for owned children | Any kernel-valid signal through `PROC_SIGNAL`; `SIGKILL` terminates (137), stop/continue set wait events, other signals queue for `PROC_SIGRECV`. Signal 0 is an existence check. A negative pid addresses a child group. `kill(0)`/`kill(-1)` and `sigqueue` are `ENOSYS` because the server has no caller pid. |
| `SETPGID` | works | A direct child owned by the connection; `pgid=0` means the child pid. `pid=0` is `ENOSYS` (no client pid). |
| `GETPGID` | works | Explicit positive pid through `PROC_GETPGID`; `pid=0` is `ENOSYS`. |
| `GETSID` | works | Explicit positive pid through `PROC_PID_INFO`; `pid=0` is `ENOSYS`. |
| all other ops | `ENOSYS` | `FORK`, `EXEC`, `SETSID`, `SETCRED`, `GETGROUPS`, `RLIMIT`, `RUSAGE`, `PRIO`, `SCHED`, `SETTITLE`, `PRCTL`, `LOGIN`, `PLEDGE`, `CAPENTER`, `SESSIONINFO`. The codec recognises them so a client gets a clean error, not a protocol mismatch. |

## Spawn attributes (P2n)

`SPAWN` accepts `POSIX_SPAWN_SETPGROUP`, `POSIX_SPAWN_SETSIGMASK`, `POSIX_SPAWN_SETSIGDEF`, the
`S_USE_PATH_HINT` and `S_CLOEXEC_DEFAULT` hints, an fd map and the three 1024-bit signal sets.
`RESETIDS`, `SETSID`, the scheduler bits and `tcpgrp_fd` remain `ENOSYS`.

### Descriptor map

The client transfers each source handle into the `proc:` server with `HANDLE_TRANSFER_FILE`
(`raven_scheme::Client::transfer_to_server`) and puts the returned server-side handle value in
`FdMapEntry { fd, handle }`. Before creating a process, procd:

* caps the map at `raven-proto-proc::MAX_FDS`;
* rejects a negative fd, fd >= 1024, a duplicate destination fd, `Handle::NONE`, or a handle
  without `TRANSFER` (a self-transfer probe), all before `PROC_CREATE`;
* closes the transferred source handles after success or failure.

The runtime loader (`userland/rt/src/spawn_attrs.rs`) transfers every source handle into the child
with `HANDLE_TRANSFER`, packs the child handle values into the `RFDM` block (version 1, 18-byte
entries, `OfdKind::Unknown`, `O_RDWR`, no path), maps it below the initial stack and publishes
`AT_RAVEN_FDMAP`/`AT_RAVEN_FDMAP_N`. The child descriptor table starts empty, so a descriptor not
in the map is absent; `S_CLOEXEC_DEFAULT` is therefore the default behaviour. The libc `start.rs`
consumer of `AT_RAVEN_FDMAP` is P9c and is not part of P2n.

### Process group and session

`SETPGROUP` maps to `PROC_SETPGID` after the spawn: `pgroup=0` selects the child pid; a positive
group must exist in the session. `SETSID` is `ENOSYS` because the kernel's `PROC_SETSID` operates
only on the caller and there is no setsid-by-handle operation.

### Signal sets

The three sets are validated against signals 1..=64; blocked and ignored sets may not contain
`SIGKILL` or `SIGSTOP`, and a non-empty `sigdefault` without `SETSIGDEF` is `EINVAL`. The loader
maps the sets and publishes `AT_RAVEN_SIGMASK` (0x7008), `AT_RAVEN_SIGDEFAULT` (0x7009) and
`AT_RAVEN_SIGIGNORE` (0x700A). The kernel keeps no process signal mask; libc owns the user mask,
so applying these keys in libc `start.rs` is a follow-up outside P2n. The `proc-spawn` fixture
reads the keys directly and proves the procd/loader contract.

| Auxiliary key | Value |
|---|---|
| `AT_RAVEN_FDMAP` (0x7003) | Child address of the `RFDM` block. |
| `AT_RAVEN_FDMAP_N` (0x7004) | Block entry count. |
| `AT_RAVEN_SIGMASK` (0x7008) | Child address of the initial blocked set. |
| `AT_RAVEN_SIGDEFAULT` (0x7009) | Child address of the default-disposition set. |
| `AT_RAVEN_SIGIGNORE` (0x700A) | Child address of the ignored set. |
| `AT_RAVEN_ABI` (0x7006) | ABI version when an attribute page is present. |

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
| `process.posix-spawn` | waived | Native libc spawn exists but its QEMU case needs a built target sysroot; procd is not wired into libc (P17). P2n supplies the procd fd-map/pgroup/signal contract. |

## Evidence

```
$ cargo nextest run -p raven-proto-proc
7 tests, 7 passed, exit 0

$ cargo nextest run -p raven-rt
91 tests, 91 passed, exit 0

$ cargo nextest run -p raven-procd
16 tests, 16 passed, exit 0

$ cargo xtask qtest --case proc-fork --arch x86_64
proc-fork: pass

$ cargo xtask qtest --case proc-exec --arch x86_64
proc-exec: pass

$ cargo xtask qtest --case proc-procd-signal --arch x86_64
proc-procd-signal: pass

$ cargo xtask qtest --case proc-spawn --arch x86_64
proc-spawn: pass

$ cargo xtask qtest --case proc-spawn --arch aarch64
proc-spawn: pass

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

`proc-spawn` serial evidence (x86_64; aarch64 is identical apart from the loader banner):

```
procd-test: hello version=1 pid=3
procd-test: attrs invalid=6
procd-test: attrs spawn1=4
procd-child: attrs fd=1 cloexec=1 pgid=1 sig=1
procd-child: ok
process: child exited (0)
procd-child: attrs fd=1 cloexec=1 pgid=1 sig=1
procd-child: ok
process: child exited (0)
procd-test: attrs spawn2=5 pgid=1
procd-test: ok
init exited (0)
```

The child reads fd 7 from the transferred pipe (`ok\n`), proves fd 8 is absent under
`S_CLOEXEC_DEFAULT`, checks its self group and the explicit peer group, and verifies the three
signal-set bits. The six rejected requests (duplicate, negative and out-of-range fd, unknown
handle, `SIGKILL` mask, `sigdefault` without `SETSIGDEF`) leave no child for `WAIT ... WNOHANG`.

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
* **No fd transfer to libc yet.** procd and the runtime loader install the child handles and the
  `RFDM` block; libc's `start.rs` consumer is P9c and is not merged in P2n's base.
* **Signal application in libc.** The signal sets reach the child auxiliary vector, but libc
  `start.rs` has no consumer yet, so a libc program does not install the initial mask. P9c owns
  that file.
* **No spawn-time session.** `POSIX_SPAWN_SETSID` and `tcpgrp_fd` are `ENOSYS`; the scheduler
  spawn bits remain `ENOSYS` too.
* **No boot integration.** Profiles/service manifests are outside P2e's owns; the fixture starts
  `procd` from its initfs. A profile service entry is a later change.
* **P3.** Scheduler stop/resume, controlling terminals, termios and pty job control remain P3.
* No board claim.
