# Shared submission/completion rings

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (the pure core of `architecture/kernel-next/01-ipc-and-handles.md` R-IPC-21..22).
This file describes `crates/raven-ring`; the kernel `Ring` object, the wire `SqEntry`/`CqEntry`
layout and buffer registration are Q6.

## 1. What this is

Q1 models channels (call/reply state). This is the next layer: the fixed-capacity ring that carries
descriptors between the kernel and user space on the bulk-data path. It is the pure, dependency-free
core of the ring; it owns no memory and knows nothing about the kernel or the wire format.

## 2. Model

A [`Ring`] is a single-producer/single-consumer queue over caller-provided storage:

* The caller hands it a `&[AtomicU64]` slot slice (a power-of-two length) once.
* The ring owns only the free-running `head`/`tail` counters and a poison flag, so the fast path
  never allocates (`R-IPC-21`: the kernel owns the doorbells and the lifetime; descriptors are
  shared memory).
* [`Descriptor`] packs a 63-bit value and a cancelled bit into one word; Q6 maps the wire entry
  onto consecutive words.

## 3. Index discipline (`R-IPC-22`)

Free-running `u64` counters, masked for indexing — the same discipline `raven-ringbuf` uses. The
capacity is exactly the storage length (no sentinel slot).

* Producer: write the slot `Relaxed`, then publish `head` with `Release`.
* Consumer: load `head` with `Acquire` (orders every prior slot write), read the slot, publish
  `tail` with `Release`.
* Producer observes `tail` with `Acquire` to detect full.

Full is `head - tail >= capacity`; empty is `head == tail`.

## 4. Operations

| Operation | Semantics |
|---|---|
| `submit` | producer push; `Full` when full, `Poisoned` once poisoned |
| `complete` | consumer pop, oldest first; `None` when empty |
| `submit_batch` / `complete_batch` | bounded batch forms |
| `cancel(value)` | mark pending descriptors with `value` cancelled (the consumer still pops them and observes the bit) |
| `poison` | terminal error state: submits fail, the consumer drains the remainder |

Cancellation never removes or reorders entries: the consumer pops the slot and reports the
cancelled bit, so the queue stays FIFO and the counters stay consistent.

## 5. Limits

* Capacity is the caller's storage length and must be a non-zero power of two (`InvalidCapacity`
  otherwise); there is no upper bound beyond the caller's storage.
* Descriptor values are 63-bit (the top bit is the cancelled flag).
* The ring is SPSC per half (`R-IPC-22`); a multi-producer submission ring is serialized by the
  producer side in Q6, not here.

## 6. Test coverage

* Wrap (slot reuse across counter wrap), full/empty, batch, cancellation (pending vs already
  completed), poison, invalid capacity, descriptor packing.
* An interleaved reference model: every operation sequence up to depth 6 over a capacity-2 ring is
  replayed against a `VecDeque` reference and the observable state must agree.
* A million-operation fill/drain model proving order and wrap (slots reused ~1000 times).
