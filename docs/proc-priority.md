<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Process priority and scheduler queries

P2l adds the POSIX priority surface behind `process.priority`: `getpriority`, `setpriority`,
`nice`, `sched_get_priority_max/min`, `sched_setscheduler` and `sched_getscheduler`. The ABI
contract crate is unchanged; the calls use kernel-owned raw numbers next to `PIPE`/`PROC_EXEC`.

## Raw calls

| Number | Call | Arguments |
|---|---|---|
| `0x78` | `getpriority` | `(which, who, out: *mut i32) -> 0` |
| `0x79` | `setpriority` | `(which, who, nice: i32) -> 0` |
| `0x7a` | `nice` | `(inc: i32, out: *mut i32) -> 0` |
| `0x7b` | `sched_get_priority_max` | `(policy) -> priority` |
| `0x7c` | `sched_get_priority_min` | `(policy) -> priority` |
| `0x7d` | `sched_setscheduler` | `(pid, policy, priority) -> 0` |
| `0x7e` | `sched_getscheduler` | `(pid) -> policy` |

The signed nice value is written through an out pointer, so a successful `-1` is a value and
never collides with the kernel's `-errno` syscall convention. `userland/rt/src/priority.rs`
exposes the corresponding safe wrappers and the `PRIO_*`/`SCHED_*` constants.

## Nice and scheduler levels

`Process` stores the exact POSIX nice value in `-20..19`. The fair scheduler has 32 run-queue
levels (0 highest, 31 lowest); `process/priority.rs` maps nice to a level with nice 0 at the
default level 16, so a process that never changes priority keeps its previous placement:

| nice | level |
|---|---|
| -20 | 0 |
| -10 | 8 |
| -1 | 15 |
| 0 | 16 |
| 1 | 16 |
| 10 | 23 |
| 19 | 31 |

User threads derive their effective run-queue priority from the owning process's nice value at
every enqueue/wake, so `setpriority` also reaches threads that were blocked and later wake. A
thread already sitting in a run queue keeps its slot until it is next enqueued; the qtest is
single-threaded. `Process.nice` is inherited by `PROC_FORK` and preserved by exec.

## Permissions and semantics

* Root (`euid == 0`) may change any process's nice within the full range.
* An unprivileged caller may change a process with the same real/effective uid pair and may only
  increase its nice (lower its priority). Raising priority returns `EPERM`.
* `PRIO_PROCESS` with `who == 0` means the caller; `PRIO_GROUP` uses the caller's process group;
  `PRIO_USER` uses the caller's real uid. Group/user changes are checked atomically before any
  process is modified.
* `getpriority` returns the lowest nice (highest priority) among the selected processes and
  applies the same same-user/root rule. An invalid selector is `EINVAL`; no live target is
  `ESRCH`.
* `nice(inc)` clamps to `-20..19`; an unprivileged negative increment is ignored (the POSIX
  "decrease is not permitted" case) and returns the current value. A negative `setpriority`
  attempt by an unprivileged caller is `EPERM`.

## Scheduler support

The fair scheduler does not implement real-time classes. The surface is explicit:

* `SCHED_OTHER` is the only honourable policy; `sched_get_priority_max/min(SCHED_OTHER)` return
  `0`, `sched_getscheduler` returns `SCHED_OTHER`, and `sched_setscheduler(pid, SCHED_OTHER, 0)`
  succeeds (self only for an unprivileged caller; root may target another process).
* `SCHED_FIFO` and `SCHED_RR` return `ENOTSUP` from both range queries and
  `sched_setscheduler`; no policy is faked. Unknown policies return `EINVAL`; a non-zero
  `SCHED_OTHER` priority returns `EINVAL`.
* Querying another user's scheduler state returns `EPERM`.

## Exposure

`userland/rt` is the exposed client. The kernel `PROC_*` priority calls are the contract; there is no userspace process server.

## Tests

```
cargo xtask qtest --case proc-priority --arch x86_64
cargo xtask qtest --case proc-priority --arch aarch64
```

The fixture runs the root get/set/clamp/nice, group/user and scheduler checks, then creates an
empty root child, drops to uid 1000 and proves the `EPERM` denials for raising its own nice,
changing the root child, the root-containing group, and querying/changing the other user's
scheduler state. No board claim.
