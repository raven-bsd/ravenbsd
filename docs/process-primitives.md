<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel process/signal primitives (P2f)

Status: the raw syscall surface, wait-status encoding, signal queue, process
groups/sessions, times/getrusage and rlimits are implemented and exercised by
the `proc-signal`, `proc-session` and `proc-wait` qtests. Stop/continue are
reported as wait events; actual scheduler suspension is a follow-up. Signal
delivery to the libc pending set is not wired (the kernel queue is read through
`PROC_SIGRECV`). User CPU time is not sampled yet; `times`/`getrusage` report
accumulated syscall time as system time and zero user time.

The ABI contract crate is unchanged. The new calls use the unused native
process-control range `0x48..=0x4F`, coordinated with P2e:

| Number | Call | Arguments | Result |
|---|---|---|---|
| `0x48` | `PROC_WAIT_EX` | `idtype, id, options, out_status` | child pid, 0 for `WNOHANG`, or errno |
| `0x49` | `PROC_GETPGID` | `pid` (0 = caller) | process-group id |
| `0x4A` | `PROC_SIGNAL` | `target, signo` | 0 |
| `0x4B` | `PROC_SIGRECV` | `out, flags` | signal number, `EAGAIN` for `WNOHANG` |
| `0x4C` | `PROC_TIMES` | `out, count` | 0; four u64 words |
| `0x4D` | `PROC_GETRUSAGE` | `who, out, count` | 0; eight u64 words |
| `0x4E` | `PROC_GETRLIMIT` | `resource, out` | 0; `[cur, max]` |
| `0x4F` | `PROC_SETRLIMIT` | `resource, cur, max` | 0 |

`PROC_WAIT` (`0x31`) keeps its blocking exit-code behavior and handle argument.
`PROC_SETPGID` (`0x36`) and `PROC_SETSID` (`0x37`) are unchanged.

## Wait id types and options

* `P_PID=1`, `P_PGID=2`, `P_ALL=3`.
* `WNOHANG=1`, `WUNTRACED=2`, `WCONTINUED=4`.
* A wait is restricted to the caller's direct children. `P_PID` selects one;
  `P_PGID` selects children whose `pgid` matches (0 means the caller's group);
  `P_ALL` selects any child, so the native `wait(-1)` call is `P_ALL` with
  `id=0`.
* A blocking wait sleeps on the caller's child wait queue and is woken by an
  exit, stop or continue notification. `WNOHANG` returns 0 when no event is
  ready. No matching child is `ECHILD`.
* An exited child is retained as a zombie until `PROC_WAIT_EX` consumes its
  status, even after the parent closes its process handle. The wait removes the
  pid from the registry and releases the zombie reference. A second wait for
  that pid is `ECHILD`.
* When a parent exits, every live child is reparented to PID 1 (init) without
  changing the child's own pid, group or session. A later child exit queues
  `SIGCHLD` to init and wakes its wait queue, so init can reap the orphan.
* The legacy handle `PROC_WAIT` returns the exit code and removes the pid entry
  after the copy; it does not add a second wait protocol.

## Status encoding

| Event | Status |
|---|---|
| Exited with `code` | `(code & 0xFF) << 8` |
| Stopped on `signo` | `((signo & 0x7F) << 8) \| 0x7F` |
| Continued | `0xFFFF` |

Exit status is reaped once by `PROC_WAIT_EX`; a stopped/continued report clears
that event flag. The legacy handle `PROC_WAIT` returns the exit code and removes
the pid entry after the copy; the new `PROC_WAIT_EX` reap marker is the POSIX
path. A process that exits without any waiter stays a zombie until init (or its
reparented parent) waits for it.

## Signal queue

`PROC_SIGNAL` accepts `target > 0` (pid), `0` (caller's group) or a negative
`-pgid`. It requires effective uid 0 or a real/effective/saved-uid match with
the target; no ambient rights. `SIGKILL` terminates the target (exit code
`128 + 9`), stop signals set the wait-status stop state, and `SIGCONT` clears it
and queues `SIGCONT`. Other signals are queued as `[signo, sender_pid, 0]` and
read with `PROC_SIGRECV`. A child exit queues `SIGCHLD` to its parent with the
child pid as the sender.

The queue is per process. `PROC_SIGRECV` blocks on the queue unless `WNOHANG`;
an empty no-wait call is `EAGAIN`. The existing per-thread `SIG_SETUP`/
`SIGRESEND` entry path is unchanged; queued signals are not yet injected into
libc's user pending set.

## Groups and sessions

`PROC_SETPGID`/`PROC_SETSID` already existed. `PROC_GETPGID` returns the stored
group; `PROC_SIGNAL` to a negative id walks the process-group members. Session
membership remains the `sid` field on `ProcessIds`; a group may be joined only
inside the target's session (the existing rule).

## Times, rusage, rlimits

`PROC_TIMES` writes `[user_ns, system_ns, child_user_ns, child_system_ns]`.
`PROC_GETRUSAGE(0, ...)` writes the caller's eight-word record; `who=-1` uses
the reaped children's accumulated time. The current slice charges syscall time
to `system_ns`; user time is always zero. `maxrss`, fault counters and context
switches are reserved zero.

`PROC_GETRLIMIT`/`PROC_SETRLIMIT` use 16 resources (`RLIMIT_CPU=0` through the
POSIX slots; `RLIMIT_NPROC=6`). Limits are inherited by `PROC_CREATE`/`PROC_FORK`.
`RLIMIT_NPROC` is enforced: a create/fork returns `EAGAIN` when the real uid
already owns that many live processes (the caller counts). Other resources are
stored and returned; enforcement
for `NOFILE`, `AS`, `CORE` and the rest belongs to the consumers/follow-ups.
`cur > max` is `EINVAL`; raising the soft limit above the hard limit without
effective uid 0 is `EPERM`.

## Kernel stack

P2i raised the shared kernel stack from five to seven mapped pages (28 KiB plus the guard).
P2g measured `PROC_FORK`/`PROC_SIGNAL` at 24,816 bytes including the 288-byte aarch64 trap frame;
six pages would still be short, while seven restore about 3.9 KiB of headroom. The same constant
is used by x86_64, where the current cases were shallower but the latent depth is the same.

## Tests

```sh
cargo nextest run -p raven-proc-primitives
cargo nextest run -p raven-ipc
cargo check -p raven-kernel --target x86_64-unknown-none
cargo check -p raven-kernel --target aarch64-unknown-none-softfloat
cargo xtask qtest --case proc-signal --arch x86_64
cargo xtask qtest --case proc-session --arch x86_64
cargo xtask qtest --case proc-wait --arch x86_64
cargo xtask qtest --case proc-wait --arch aarch64
make check-fast
```

The host library compiles the same `kernel/src/process/status.rs` source and
tests the status/stop/continue encodings, signal bounds and rlimit defaults.
The signal qtest covers two queued signals, `SIGCHLD`, `EAGAIN` and invalid/
unknown-target errors. The session qtest covers groups, sessions, stop/continue
statuses, `P_PID`/`P_PGID`/`P_ALL`, kill statuses, times/rusage, rlimits and
`RLIMIT_NPROC` enforcement. The `proc-wait` qtest covers `waitpid`, `wait(-1)`,
`WNOHANG`, double-wait `ECHILD`, stop/continue and an orphan that is reparented
to init after its parent exits. Existing `proc-identity`, `proc-registry`,
`proc-handle-info`, `proc-credentials` and `proc-fork` remain green.

## Limits / follow-ups

1. **No scheduler suspension.** `SIGSTOP`/`SIGCONT` set/clear the wait-status
   state but do not stop the target's threads; P3 job control needs a scheduler
   stop/resume seam.
2. **No libc pending-set injection.** Queued signals are consumed by
   `PROC_SIGRECV`; the existing libc signal-entry path still owns its own
   pending set. A future libc signal-entry integration can consume the queue directly.
3. **User time and rusage counters.** Only syscall time is accumulated; user
   time, RSS and fault/context-switch counters are zero until scheduler and MM
   accounting hooks land.
4. **Zombies and reparenting.** An exited child is held as a strong zombie until
   `PROC_WAIT_EX` (or the legacy handle `PROC_WAIT`) reaps it, independent of the
   parent's handles. A parent exit reparents live children to PID 1, whose wait
   consumes them. A parent that never waits therefore keeps a zombie by design;
   init is the eventual reaper for orphaned children.
5. **No board claim.** All evidence is host/QEMU.

## Provenance

No donor code was read or copied. The syscall/status code and fixtures are
first-party BSD-3-Clause work.
