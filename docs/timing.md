<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Timing: cyclics, the next-deadline rule and tickless idle

Status: PROPOSED. Implements the pure half of `kernel-next/04` R-TIM-4/5/10 in `crates/raven-cyclic`
(L2); the kernel integration (`cyclicd`, per-CPU heaps, hardware programming) is L4.

## Deadline model

`crates/raven-cyclic` (no_std + alloc, `#![forbid(unsafe_code)]`, no kernel dependency) models one
CPU's timers as a binary min-heap keyed by the absolute fire deadline `when` (equal deadlines fire
in id order). A [`Backend`] trait (`program(deadline)`, `now_ns`) is the hardware seam.

* `add(spec) -> CyclicId` inserts and reprograms the backend if the new deadline is earlier than
  the one already programmed.
* `cancel(id) -> bool` is lazy: it marks the cyclic cancelled, which is skipped and reclaimed when
  its deadline arrives; cancelling an unknown or already-cancelled id is a no-op.
* `on_interrupt(now, fire)` drains every due, non-cancelled cyclic in deadline order — the "ordered
  merge" of R-TIM-10, so every deadline reached since the previous interrupt coalesces into one
  pass — re-inserting periodic cyclics at `when + interval`, then reprograms the backend to the
  earliest remaining deadline (or disarms it).
* `next_deadline()` exposes the next fire time; the backend is reprogrammed only when it changes.

A cyclic carries `level` (Low/Normal/High, for interrupt-vs-thread handler context per R-TIM-5)
and `slack_ns` (coalescing hint per R-TIM-10). This crate owns the pure ordering, not the handler
dispatch or the hardware.

## The next-deadline rule

The heap root is the earliest deadline. When it is due, `on_interrupt` pops and fires it and
re-programs the backend for the new root; when it is in the future, the backend stays armed for
that deadline and nothing fires until then. There is no periodic tick below the layers that ask
for one.

## Tickless behaviour

* An empty heap programs the backend to `None` (disarmed): an idle CPU halts (`hlt`/`wfi`) instead
  of ticking.
* With one far deadline, the backend is armed exactly once for that deadline and no intermediate
  interrupt fires before it (the idle-tick test).

## Fire path

`on_interrupt` performs no allocation: it only pops due entries and re-pushes periodic ones, which
keeps the backing vector within its existing capacity. `add` is the only allocating operation.

## Verification

`cargo nextest run -p raven-cyclic` runs: deadline-order fire, equal-deadline id order, one-shot and
periodic behaviour, cancel safety/idempotence, backend reprogramming on add/fire/cancel, idle-tick
(empty halts, far deadline armed once), coalesced ordered merge, a slack/level carry check, and a
2000-step randomized insert/cancel/fire model compared against a reference min-heap.
