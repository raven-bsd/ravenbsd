<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Locking: turnstiles and priority inheritance

Status: PROPOSED. Implements the pure half of `kernel-next/04` R-LCK-5 and R-LCK-6 in
`crates/raven-turnstile` (L1); the kernel glue (`sync.rs` blocking `Mutex`, `RwLock`, `Condvar`)
lands later.

## What a turnstile is

A turnstile is the queue of threads blocked on one lock, keyed by the lock/object id, ordered by
effective priority then arrival, plus the inheritance link to the owner. The owner is the thread
that currently holds the lock; waiters are blocked until the owner wakes them. Release hands the
lock directly to the highest-priority waiter (direct handoff), so a newly woken high-priority
waiter does not race a lower-priority one.

## Priority convention

Priorities follow `raven-sched-core`: `0` is the highest, larger numbers are lower. A thread's
**effective** priority is the most urgent of:

1. its base priority, and
2. the effective priorities of every thread waiting on a lock it owns, transitively.

This is the least fixed point of `eff(t) = min(base(t), { eff(w) | w waits on a lock owned by t })`.
Because priorities only rise toward `0`, the fixed point is unique and is reached by iterating to
stability (OBSERVED in the crate's `compute_effective`). Transitive inheritance satisfies R-LCK-6:
if `A` blocks on a lock owned by `B`, and `B` blocks on a lock owned by `C`, then `C` inherits
`A`'s priority.

## Operations

| Operation | Effect |
|---|---|
| `acquire(object, thread)` | Set `thread` as owner; fails if already owned or thread unknown. |
| `block(object, thread)` | Enqueue `thread` at its effective priority; may raise the owner. |
| `wake_one(object)` | Hand the lock to the highest-priority waiter; release if none. |
| `wake_all(object)` | Release and wake every waiter, priority first. |
| `cancel(object, thread)` | Remove a waiter without waking it; recompute the owner. |
| `boost(object)` | Recompute and report the owner's inheritance effect (idempotent). |
| `unboost(object)` | Drop the contribution; requires no waiters; reports restoration. |

`block`/`wake`/`cancel` maintain inheritance automatically; `boost`/`unboost` are the explicit
recomputation points the kernel calls at the owner boundary and report an `InheritEffect`.

## Invariants

The crate's `check_invariants` verifies, and the exhaustive model check exercises:

1. An owner never appears in its own waiter list.
2. A waiter list has no duplicate thread and is sorted by `(priority, arrival)`.
3. A thread waits on at most one object.
4. The owner is boosted to the highest waiter (its effective priority is the most urgent of its
   base and all waiters on locks it owns).
5. The owner is restored to its base priority when the last waiter on its last held lock leaves.
6. Cancellation removes exactly one waiter and loses no other waiter.
7. A waiter is woken at most once.
8. No priority inversion: while a high-priority thread waits, the owner runs at a priority at least
   as urgent as every merely-runnable lower/equal thread, so none of them preempts the owner.

## Model-check bounds

Exhaustive breadth-first search over the real transition functions:

* 2 threads, 2 objects, 2 priority levels, depth 6;
* 3 threads, 1 object, 2 priority levels, depth 7.

The search reports a reproducible state count and fails on the first violated invariant with a
counterexample trace. Two seeded-bug tests prove the checker flags a planted `EffectiveMismatch`
and a planted `WaiterOrdering` (R-ADD-28 style).

A deterministic multi-million-operation soak (`storm_millions_of_mixed_operations`, 3M mixed
wait/wake-one/wake-all/cancel/acquire/boost/unboost with object and thread reuse plus stale ids)
asserts, at every step, the conservation invariant `blocks == wakes + cancels + queued` (no lost or
double-counted wake) and re-runs the full invariant check periodically. Two targeted tests cover a
32-owner deep inheritance chain (the deepest owner inherits the highest priority transitively, and
every intermediate owner is boosted to the same value) and equal-priority fairness (64 waiters hand
off in arrival order).

## Lock statistics (`lockstat` feature)

`crates/raven-turnstile` has an off-by-default `lockstat` feature. When enabled, each turnstile
carries per-object counters (`Turnstiles::stats(object) -> LockStats`) that the kernel `lockstat`
probes will publish (L5). The default build has no counters and no extra cost or layout.

| Counter | Meaning |
|---|---|
| `wait_entries` | successful `block` calls |
| `immediate_acquisitions` | successful `acquire` calls |
| `wakes` | threads woken (`wake_one` + `wake_all`) |
| `cancels` | waiters removed by `cancel` |
| `timeouts` | reserved for the kernel timeout path; always `0` here |
| `max_queue_depth` | largest number of simultaneous waiters |
| `max_inheritance_depth` | longest inheritance chain observed (the block hop plus the owner's own wait-for chain) |
| `inherited_priority_changes` | number of times an operation changed an owner's effective priority |

Every counter is saturating (never wraps silently). Equality/hashing of the model state ignores the
counters, so the exhaustive model check still deduplicates on the logical state; the same model
additionally asserts `timeouts == 0`, `max_queue_depth >= current waiters` and
`wait_entries >= current waiters` on every reachable state under the feature.

## Kernel integration (later)

The kernel hosts the real threads and calls these operations from the blocking mutex/rwlock/condvar
paths, applying each `InheritEffect` to the scheduler. The global recomputation may be replaced by
incremental propagation along the wait-for chain (bounded depth 8) as long as it yields the same
effective priorities. Deadlock detection and the IPC donation edges (R-LCK-7) are separate tickets.
