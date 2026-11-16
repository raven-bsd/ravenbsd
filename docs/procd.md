<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# procd process-model server (P2e)

OBSERVED on `stuart/P2e` (base `origin/master` `7a54a110`). The server, protocol and fixture are
tested; the kernel seam P2f is **not merged at this base**, so the target implementation is an
honest spawn/wait/kill/group subset rather than the complete POSIX process model. No board claim.

## Verdict

`procd` registers `proc:` and accepts version-1 `CALL` requests on an opened connection. It keeps
a table only of the children **it created**; the kernel remains authoritative for live identity,
address spaces and exit timing. The server never invents a process, never claims a pid it does not
own, and answers `ENOSYS` for a row whose kernel primitive is absent.

The host model (`raven-procd`) and the wire codec (`raven-proto-proc`) have unit tests. Two QEMU
cases exercise the real setuid-independent path:

* `proc-exec` spawns `/bin/procd-child` with argv/env, returns the pid, waits, and reaps exit 7.
* `proc-signal` spawns a sleeper, sets its process group, reads pgid/sid, maps SIGTERM to
  `PROC_TERMINATE`, and reports a POSIX signal-style wait status.

## Transport and wire format

* One connection is an `OPEN proc:` file; `CALL` carries the request and response. A server-chosen
  connection id is retained by `procd`; a call on an unknown id is `EBADF`.
* All integers are little-endian. Four-byte fields align to four and eight-byte fields to eight;
  strings are `u32` length plus bytes and padding; argument vectors are `u32 count`, `u32 total
  bytes`, NUL-terminated entries and padding.
* `RequestHeader { version: u16 = 1, op: u16, flags: u32, seq: u64 }` and
  `ResponseHeader { status: i32 = -errno, reserved: u32, value: i64 }` are each 16 bytes.
* `raven-proto-proc` owns the codec. The operation numbers match the libc donor-map sketch
  (`HELLO=0x01 .. SESSIONINFO=0x16`); unknown numbers are `EINVAL`, a wrong version is `EPROTO`.
* The response `status` is `0` or `-errno`; `value` is the primary scalar (pid, pgid, sid, count).

## Implemented operations

| Op | State | Behaviour |
|---|---|---|
| `HELLO` | works | Returns server version 1 and the `procd` pid. |
| `SPAWN` | works, bounded | Spawns an **initfs** image with UTF-8 argv/env. `POSIX_SPAWN_SETPGROUP` is applied through the kernel; `pgroup=0` means the child pid. A fast-exit child falls back to the server's group/session for the record. Rejects with `ENOSYS` if signal masks, scheduler fields, `tcpgrp_fd` or an fdmap are requested; rejects with `EINVAL` for a relative path, empty argv, NUL or invalid UTF-8. `umask` is recorded but the kernel has no child-umask operation. |
| `WAIT` | partial | `P_PID` + blocking only. Returns the pid and a POSIX wait-status word: exit `(code & 0xff) << 8`, or the signal number for a child terminated through this server. `WNOHANG`, `P_ALL` and stop/continue are `ENOSYS`. `siginfo`/`rusage` are zero. A second wait is `ECHILD`. |
| `KILL` | partial | `SIGINT`, `SIGKILL` and `SIGTERM` on a child owned by the connection map to `PROC_TERMINATE(handle, 128+sig)` and mark the child for signal-style wait status. Signal 0 is an existence check. A negative pid addresses the matching child groups. Unsupported signals, `kill(0)`, `kill(-1)` and signal queues are `ENOSYS`; there are no dispositions or pending sets. |
| `SETPGID` | partial | A direct child owned by the connection; `pgid=0` means the child pid. `pid=0` (the caller) is `ENOSYS` because the server does not receive a client pid from the kernel yet. |
| `GETPGID` / `GETSID` | partial | Explicit positive pid through `PROC_PID_INFO`, so a procd child or another live process can be queried. `pid=0` is `ENOSYS`; there is no client-pid stamp. |
| all other ops | `ENOSYS` | `FORK`, `EXEC`, `SETSID`, `SETCRED`, `GETGROUPS`, `RLIMIT`, `RUSAGE`, `PRIO`, `SCHED`, `SETTITLE`, `PRCTL`, `LOGIN`, `PLEDGE`, `CAPENTER`, `SESSIONINFO`. The codec recognises them so a client gets a clean error, not a protocol mismatch. |

## Process matrix

The row evidence is the repository's current POSIX gate plus the P2e Qtests. A `Waived` row names
the missing primitive; P2f/P3/P17 own those seams.

| Row | State | Evidence / waiver |
|---|---|---|
| `process.process-id` | present | Kernel pid registry and `PROC_PID_INFO`/`PROC_HANDLE_INFO`; existing `proc-registry` and `proc-identity` cases. |
| `process.fork` | present (kernel), procd op waived | Existing `proc-fork` qtest exercises `PROC_FORK`; the server cannot fork a client because the kernel has no process-handle transfer to procd yet (K-5/P2f). |
| `process.exec` | spawn half present, replacement waived | `proc-exec` proves image spawn with argv/env; `PROC_RESET`/same-pid image replacement is absent (K-2/P2f), so `EXEC` is `ENOSYS`. |
| `process.wait` | partial | `proc-exec`/`proc-signal` prove blocking `P_PID` wait and status encoding; `WNOHANG`, `P_ALL`, stop/continue and `waitid` need P2f. |
| `process.signal` | partial | `proc-signal` proves SIGTERM to a procd child and signal-style status; dispositions, masks, pending queues, SIGCHLD and arbitrary-pid delivery need P2f. |
| `process.terminate` | partial | `KILL`/`PROC_TERMINATE` for owned children; no external signal permission model. |
| `process.process-group` | partial | `SETPGID`/`GETPGID` for owned children; group signalling only for child groups; no external group table. |
| `process.session` | waived | A child session cannot be created with the current self-only `PROC_SETSID`; procd does not call `setsid` for itself. P2f/P3. |
| `process.credential` | partial | Kernel `setresuid`/`setresgid`/groups exist (P2c), but `SETCRED` on a client/child is not exposed by the process handle; the server rejects the op. U3a/U3b own tool-level credential transitions. |
| `process.times` | waived | No kernel CPU-time accounting (`CLOCK_GET` ids 2/3, `PROC_STATS`); `RUSAGE` is `ENOSYS` and wait rusage is zero. P2f. |
| `process.resource` | waived | No rlimit syscall; `RLIMIT` is `ENOSYS`; server defaults are not claimed as enforcement. P2f. |
| `process.priority` | waived | No scheduler priority syscall; `PRIO`/`SCHED` are `ENOSYS`. |
| `process.system` / `process.popen` | waived | No pipe primitive and no procd client in libc yet; a shell/`Command` path is P17/P3. |
| `process.job-control` | waived | Needs stop/continue, controlling terminal and `ttyd`/`tcsetsid`; P2f/P3. |
| `process.tty-control` | waived | `ttyd` and `SESSIONINFO` are not implemented; P3. |
| `process.posix-spawn` | partial | Libc has the native spawn path; this ticket provides the server-side `SPAWN` protocol but does not edit libc to use it. P17. |

`posix-check` currently reports `process.fork` as the only executable process row; the other rows
are `fail` in the generated report because the P0b evidence overlay (dependency, not merged) owns
the row status and executable-evidence mapping. P2e supplies the QEMU cases and this matrix but
does not edit the shared profile/overlay.

## Evidence

```
$ cargo nextest run -p raven-proto-proc
7 tests, 7 passed, exit 0

$ cargo nextest run -p raven-procd
7 tests, 7 passed, exit 0

$ cargo xtask qtest --case proc-exec --arch x86_64
proc-exec: pass, exit 0

$ cargo xtask qtest --case proc-signal --arch x86_64
proc-signal: pass, exit 0

$ cargo xtask qtest --case proc-fork --arch x86_64
proc-fork: pass, exit 0
```

`proc-exec` serial evidence:

```
procd-test: hello version=1 pid=3
procd-child: exec TEST=value
process: child exited (7)
procd-test: spawn pid=4
procd-test: wait exit=7
procd-test: unknown=echild
procd-test: ok
init exited (0)
```

`proc-signal` serial evidence:

```
procd-test: hello version=1 pid=3
procd-child: sleeping
procd-test: spawn pid=4
procd-test: setpgid pid=4
procd-test: getpgid=4
process: child exited (143)
procd-test: kill signal=15
procd-test: wait signal=15
procd-test: unknown=echild
procd-test: unsupported=enosys
procd-test: ok
init exited (0)
```

## Limits and follow-ups

* **No kernel seam.** P2f must add wait-any/WNOHANG, stop/continue, signal queues, session and
  group membership, accounting, rlimits and a client-id stamp before `procd` can claim the full
  matrix. Until then the server must keep returning `ENOSYS`.
* **No fd transfer.** `SPAWN` rejects an fdmap; the child is initfs-only. P15/P17 own fd and
  interpreter support.
* **No client pid.** `Ctx` carries credentials only. The server enforces ownership by connection
  uid, not by a caller pid; `pid=0` operations are therefore `ENOSYS`.
* **Blocking wait.** A `WAIT` call blocks the single server loop. A deferred-wait worker is a
  follow-up once the kernel exposes non-blocking status.
* **No boot integration.** Profiles and service manifests are outside P2e's owns; the qtest starts
  `procd` from its initfs. The release/base image needs a later profile service entry.
* No board claim.
