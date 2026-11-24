# Native IPC: the channel state machine

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED (matches `architecture/kernel-next/01-ipc-and-handles.md` R-IPC-1..11). This file
describes the pure `crates/raven-chan` model; the kernel `Channel` object, the wire format and the
syscalls are built on top of it in the later integration tickets (KN-102..103).

## 1. What this is

A `Channel` pairs two endpoints (`A`, `B`) and owns only the *call/reply bookkeeping*: the
`txid` matching table, the bounded queues, the cancellation and the terminal events. It has no
wire format, no syscall numbers, no handle/rights model, no payload bytes and no scheduling.
`payload_id` is an opaque `u64` the kernel later binds to a message buffer or ring token.

The native ABI is handle-and-channel based (R-IPC-1..3); the scheme packet router is the Redox-shaped
compatibility path. This state machine is the piece that decides whether the native IPC is ours or a
renamed packet router, so it is pure, dependency-free and exhaustively tested before any kernel
object exists.

## 2. Operations and state table

| State of a call | Who knows it | Next events |
|---|---|---|
| queued (in the peer's receive queue) | caller (`outstanding=Queued`), peer queue | `recv` → received; `cancel`/`timeout` → removed; peer `close`/`died` → `PeerClosed`/`PeerDied` |
| received (peer is executing it) | caller (`outstanding=Received`), peer received table | `reply` → `Replied`; `cancel`/`timeout` → removed; peer `close`/`died` → `PeerClosed`/`PeerDied` |
| completed (in the caller's completion queue) | caller only | `pop_completion` |

The operations:

* `call(from, txid, payload_id)` — queue at `from.peer()` and record the call outstanding at `from`.
  Rejected (no state change) on: `txid == 0`, a duplicate outstanding `txid`, the in-flight budget
  (`Error::InFlightFull`), or the peer queue (`Error::QueueFull`).
* `recv(at)` — move the oldest queued call to `at`'s received table. `WouldBlock` on an empty queue
  with a live peer; `PeerClosed`/`PeerDead` when the queue is empty and no call can ever arrive.
* `reply(at, txid, payload_id)` — complete the call at its caller with `Replied`. Only a call `at`
  has received can be replied to; unknown, still-queued or already-completed `txid` → `NoSuchCall`.
* `reply_recv(at, txid, payload_id)` — `reply` then, if a call is queued, receive it in the same
  operation (the documented server loop, R-IPC-8). A failed reply dequeues nothing.
* `cancel(from, txid)` / `timeout(from, txid)` — abort a call `from` issued, wherever it is. A
  cancelled/timed-out call cannot be replied to.
* `close(at)` — endpoint close: every in-flight call on the channel completes with `PeerClosed` and
  all queues are drained. Idempotent.
* `died(at)` — owner death: every in-flight call on the channel completes with `PeerDead`.
  Idempotent.

## 3. Ordering

Calls from one endpoint to the other are delivered in send order (`R-IPC-19`). Replies for different
`txid`s may be sent in any order; each is matched by `txid` alone.

## 4. Bounds

Both directions are bounded (`R-IPC-10`):

* `receive_queue` — calls an endpoint has not received yet (default 64).
* `in_flight` — calls the caller has not reaped yet, *including* completions it has not popped
  (default 64). This is what stops a caller that never drains completions from growing kernel memory.

Limits clamp to `1..=MAX_CAPACITY` (1024); the kernel passes its own policy value.

## 5. Exactly-once completion

An accepted call is in exactly one of two places at any instant: the caller's outstanding table or
the caller's completion queue. Every completion path removes the outstanding entry *before* it
records the completion, so:

* a rejected call never completes;
* an accepted call completes exactly once;
* a reply after cancel, timeout, close or death finds no call (`NoSuchCall`).

The invariant `accepted = outstanding + pending + popped` is checked after every transition by the
exhaustive exploration.

## 6. Error contract

| `raven-chan` error | Meaning | Kernel mapping (R-IPC-11) |
|---|---|---|
| `InvalidTxid` | `txid == 0` on a call | `EINVAL` |
| `DuplicateTxid` | second outstanding call with the same `txid` | `EBUSY`/`EINVAL` |
| `InFlightFull` | caller has not reaped completions | `EAGAIN` (call blocks) |
| `QueueFull` | peer receive queue full | `EAGAIN` (call blocks) |
| `NoSuchCall` | unknown / already-completed / cancelled / timed-out `txid` | `ESRCH` |
| `WouldBlock` | receive queue empty, peer live | `EAGAIN` |
| `EndpointClosed` | the endpoint itself closed or dead | `EBADF` |
| `PeerClosed` | peer endpoint closed | `EPIPE` |
| `PeerDead` | peer owner died | `EPIPE` |

The terminal event is *defined*: `PeerClosed` from `close`, `PeerDead` from `died`. The kernel layer
may map both to `EPIPE`; the distinction is preserved here so the observability layer can tell a
clean close from a crash.

## 7. Exhaustive model

The test `exhaustive_small_model_saturates_with_exactly_once_and_no_reply_after_cancel_or_death`
explores the whole reachable transition system of a channel with three distinct `txid`s, a receive
queue of 2 and an in-flight budget of 3, with completion reaping disabled. From every state every
enabled action is applied and every resulting state's invariants are checked:

* each issued `txid` is outstanding or completed exactly once, never both;
* completions appear only on the issuing endpoint;
* `queued + received == outstanding(peer)`;
* `accepted == outstanding + pending + popped`;
* a `reply`/`reply_recv` succeeds iff the call is currently received — so no reply after
  cancel, timeout, close or death;
* `close`/`died` complete every in-flight call with the right terminal completion, exactly once.

The bound is the model size (3 calls, queue 2, budget 3), not a fixed path length; the search
terminates when no new state is produced, so every interleaving within the model is covered. The
reported state/transition counts are printed by the test.

## 8. Out of scope (later tickets)

Wire format and `MsgHeader` (`R-IPC-4..7`), syscall numbers (`0x110..0x11F`), handle movement in
flight, rights, the idle-server list and direct handoff (`R-IPC-12..18`), rings, and the scheme
compatibility façade. The `MsgHeader.trace_id` field (`R-IPC-4`) is likewise absent here: trace
correlation belongs to Q2 `raven-trace-core` and Q3 trace-context-in-IPC, which layer the field on
top of this state machine. This crate must not depend on `raven-abi` or the kernel.

## 9. Kernel scheme capture pool (compatibility path)

Status: OBSERVED (kernel `ipc::capture`). This is the resource model behind the S11 scheme
packet router, not part of the pure `raven-chan` state machine.

A cross-process `READ`/`WRITE` creates one capture. A `CALL` creates two captures: the request
buffer and the reply buffer. Each capture is a page-rounded copy of a client range mapped into
the server's reserved 4 MiB capture window.

Captures are packed by page range into the window's virtual slots. A request and its reply can
therefore share one slot when their combined pages fit; when they do not, the allocator uses
another slot. This keeps a `CALL` from consuming two whole 4 MiB slots just because it has two
buffers.

Lifetime rules:

* The request capture stays mapped until the server's `SEND` completes the request.
* The reply capture stays mapped until the reply has been written and `copy_back` has run.
* Completion copies back writable captures, then releases both ranges exactly once.
* Copy-back verifies the page-table root and each page's original frame. A normal
  `MAP`/`UNMAP`/`PROTECT` by another thread in the shared address space does not block an
  unrelated capture; a changed page-table root (`exec`) or a replaced/unmapped page does.
* A cancelled client keeps the mapping alive until the server retires or skips the request; a
  late `SEND` releases it.
* Releasing an already-free range is a no-op, so a double drop cannot hand the same pages to a
  second capture.

The `s125-capturecheck` command-line token prints the global active/high-water capture and page
counters on pool exhaustion and at process exit. The counts are evidence for leak, double-free
and capacity checks; they are not a policy retry.
