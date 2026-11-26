<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCG4 boot readiness and login stall

Date: 2026-11-18. Ticket: S127. Owner: kevin. Kernel and scheduler paths are unchanged; the fix
and the diagnostics are in `userland/init/**`.

## Failure evidence

Both campaigns ran the release image under QEMU TCG with `-smp 4`.

* S107 round 5, TCG4 run 5/10 (`build/smp-boot-soak/20261118T201237Z/tcg4/run-5.{log,stamped.log}`):
  `svcd: getty up` at 21.198 s and `svcd: all services up` at 22.034 s, then no
  `RavenBSD 0.1 (x86_64)` banner or `login:` before the 30 s timeout. No readiness failure was
  logged.
* S126 `smp-libc-concurrency` TCG4 (`build/smp-closeout/final-tcg4/tcg4/`):
  `svcd: logd up`, `svcd: consd up`, `svcd: nulld failed: readiness timeout`,
  `svcd: ramfsd failed: readiness timeout`, `vfsd: skipped /tmp on ramfs:: timed out`; the qtest
  timed out at 66.46 s and `smp-libc-concurrency: start` never appeared.

## Clock and deadline finding

The supervisor has no clock of its own. Every transition receives `now` from the caller;

* `userland/init/src/boot.rs::now_ms()` reads `raven_rt::clock_get(clock::MONOTONIC)` and converts
  it to milliseconds. `CLOCK_MONOTONIC` is the guest's TSC-derived boot clock, not host wall time,
  but it advances with the host while the guest makes no progress.
* `state.rs::start()` used to arm `ready_deadline_ms = now + ready_timeout_ms` **before** the
  worker thread existed and **before** the child process was created. The manifest default is
  5 000 ms (`crates/raven-svc-manifest/src/manifest.rs`); `nulld` and `ramfsd` use that default.
* `worker.rs::wait_for_scheme()` polls `client.open("<scheme>:")` every 10 ms and calls
  `state.rs::readiness_timed_out(now)`. On expiry the state becomes `Failed` and the worker
  terminates the still-running child with `proc_terminate(handle, 0)`.
* The worker only observes the deadline when it runs. If the host deschedules the whole TCG guest
  for seconds, the deadline expires while the guest is frozen, and the first poll after the guest
  resumes fails a child that has had no chance to run.

The S126 trace shows that shape: `nulld` registers `null:` before it logs `nulld: started`, yet
neither happened; the child was terminated by the supervisor (`process: child exited (0)`) without
ever reaching its first instruction. The next services then started in large wall-clock gaps
(`randd` at epoch 1795036673.625, `initfsd` at 1795036683.271). That is a starved guest, not a
blocked service: the child was created but not scheduled.

## Timing diagnostics

`state.rs::Runtime` now carries `ReadyTiming` (`generation`, `start_ms`, `spawned_ms`,
`first_poll_ms`, `deadline_ms`, `stalled_ms`, `finished_ms`). It is diagnostic only; no transition
reads it.

Every lifecycle notice keeps its exact text and is followed by one timing line:

```
svcd: <name> timing: generation=<n> start=<ms> spawn=<ms> first_poll=<ms> deadline=<ms> stalled=<ms> ready_at=<ms>
svcd: <name> timing: generation=<n> start=<ms> spawn=<ms> first_poll=<ms> deadline=<ms> stalled=<ms> finished_at=<ms>
```

`ready_at` is printed for `Notice::Up`, `finished_at` for `Notice::Failed` (readiness timeout or an
exit before ready). The timing line is separate on purpose: many qtests anchor the lifecycle line
with `^svcd: <name> up$`, so appending fields to that line would break unrelated cases.

Fields:

* `start` — when `start()`/`begin_restart()` selected the generation;
* `spawn` — when `spawned()` recorded the child handle;
* `first_poll` — the first scheme-readiness poll after the spawn;
* `deadline` — the current absolute deadline (spawn + manifest timeout, plus credited stalls);
* `stalled` — wall-clock time credited to unobserved poll gaps;
* `ready_at`/`finished_at` — when readiness or the timeout completed.

## Fix

Two changes inside `userland/init/**`, no manifest timeout changed:

1. **The window starts at spawn.** `start()` and `begin_restart()` leave `ready_deadline_ms` unset;
   `spawned()` arms it with `now + ready_timeout_ms`. Worker-thread creation and the initfs ELF copy
   are no longer charged to the service's readiness budget.
2. **Only observed time is charged.** The worker still polls every 10 ms. A gap between polls above
   500 ms means the worker itself did not run; the excess is added to both the live deadline and
   `stalled_ms` through `Supervisor::extend_readiness()`. A normally polled child still times out at
   the manifest deadline, and a hung child still fails after 5 000 ms of observed time. This is a
   progress-based adjustment, not a timeout increase.

`ready_timeout_ms` defaults and every `etc/services.d/*.toml` value are unchanged. There is no
retry, no waiver and no fake pass.

## Measured distribution from the preserved logs

From the five retained TCG4 stamped runs, spawn/first-output to `svcd: <name> up`:

| Run | nulld | ramfsd | randd | zerod | initfsd | Login |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 133.1 ms | 17.3 ms | 10.5 ms | 59.0 ms | 19.6 ms | 8.530 s |
| 2 | 10.1 ms | 25.3 ms | 8.9 ms | 6.2 ms | 12.2 ms | 8.573 s |
| 3 | 106.5 ms | 106.6 ms | 21.9 ms | 11.5 ms | 12.9 ms | 10.262 s |
| 4 | 37.0 ms | 20.9 ms | 6.9 ms | 13.4 ms | 12.1 ms | 8.314 s |
| 5 | **447.1 ms** | **147.1 ms** | 209.0 ms | 145.9 ms | 106.6 ms | **none** (banner stall) |

The worst observed readiness transition is 447.1 ms, less than a tenth of the 5 000 ms default.
The failures are therefore not ordinary readiness slowness; they are multi-second stalls in which
the guest executed nothing. Run 5's services still reached `up`, and the missing login banner is
the same starvation after `all services up`, not a readiness timeout.

## Controlled TCG4 repro

Not run: the host never met the ticket's idle condition. At 21:22Z `nproc=20`, load1 was 27.53
(load5 28.27) with other lanes' `xtask posix-check` and `xtask check` active; no S126-r1 process
was found. The ticket forbids starting the repro on a busy host, so the preserved logs above remain
the evidence base. The diagnostics are in the build for the next exclusive TCG4 run; a future
timeout line with `stalled=0` and a spawn that precedes the deadline would name per-child kernel
starvation and goes to Stuart (S103) with that line.

## Reference

* `userland/init/src/state.rs` — `ReadyTiming`, deadline arming in `spawned()`, `extend_readiness()`,
  `readiness_timed_out()`.
* `userland/init/src/worker.rs` — 10 ms scheme poll and the 500 ms stall threshold.
* `userland/init/src/boot.rs` — `now_ms()` and the timing log line.
* `userland/init/src/state_ut.rs` — deadline, timing, stall-credit and hung-child tests.
