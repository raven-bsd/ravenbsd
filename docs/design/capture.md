<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Scheme IPC capture lifetime

Status: S32 implementation. The design is OBSERVED in `kernel/src/ipc/capture.rs`; the
end-to-end evidence is the `capture-lifetime` qtest.

## 1. The rule

A scheme request never maps a client's frames into the server. The kernel owns the bytes for the
lifetime of the request:

1. On `READ`, `WRITE` or `CALL`, the kernel allocates zeroed kernel frames for the captured
   range and copies the client's pages into them. The server sees a virtual address in its own
   address space that points at those frames.
2. The frames live at the reserved capture window (section 2).
3. On completion of a writable capture (`READ` data, `CALL` response), the kernel copies the
   frames back into the client's pages **only if the client's mapping has not changed** since
   the request started (section 4).
4. The request object owns the frames; they are released exactly once when the request is
   retired or the server exits (section 5).

The S11 packet ABI is unchanged: `buffer_addr`/`response_addr` are still addresses in the
server's address space and `buffer_len`/`response_len` are still byte lengths. Only the backing
frames changed from client-owned to kernel-owned.

## 2. Reserved window

`memory::uspace::capture_window` reserves

```
BASE = 0x0000_6000_0000_0000
SLOT_SIZE = 4 MiB          (MAX_TRANSFER)
SLOTS = 8
SIZE = 32 MiB
```

as a zero-protection VMA in every user address space. Consequences:

* `MAP` never selects an address inside it (`find_free` sees the VMA) and a fixed `MAP` of the
  range fails with `EEXIST`.
* `UNMAP` and `PROTECT` of any range overlapping it are rejected with `EINVAL`.
* The address-space teardown never treats the frames in that range as process-owned; only the
  capture object frees them.
* The server can hold at most eight captures at once, one per slot.

## 3. Copy direction

* `WRITE` and the `CALL` request buffer are read-only for the server: the client is copied in at
  create and the server only reads the snapshot.
* `READ` and the `CALL` response buffer are writable for the server: the client is copied in as
  well (so bytes outside the requested range in the first/last page are preserved), the server
  writes the kernel frames, and completion copies whole pages back.

A server that reads or writes only within `buffer_addr..buffer_addr + buffer_len` sees exactly
the bytes the client supplied. The kernel never maps a client user address after create; the
copy-back writes only the physical frame observed at create, and only when the mapping
generation is unchanged (section 4).

## 4. Client unmap, remap and copy-back

The kernel records the client's mapping generation and the physical frame of every client page
at create. Copy-back runs at completion and only when:

* the client address space still exists;
* the *whole* address space has the same mapping generation (no map, unmap, protect or COW fault
  happened meanwhile) — this is conservative and covers an unmap followed by a remap that
  reused the same frames;
* the client's page still translates to the original physical frame and is writable.

Otherwise that page is skipped. A client that unmaps its range during a request therefore loses
the copy-back, but the server's snapshot is unaffected and the server never writes into the
client's new mapping. This is the contract: the server reads a stable snapshot; a client that
wants the result must keep the range mapped until the syscall returns.

## 5. Completion, cancellation and server death

* Normal completion (`SEND`): the kernel copies back writable captures, unmaps the kernel frames
  from the server, performs a TLB shootdown over the slot, and frees the frames.
* Client cancellation: `on_process_exit` marks the request cancelled and wakes/fails it, but
  keeps the frames mapped. A queued cancelled request is skipped by `RECV` and dropped, which
  releases its capture. A delivered request stays retained until the **server exits**; a late
  `SEND` returns `EIO` and does not release it, because this ticket's Owns set excludes the
  router retirement hook. A server still touching a cancelled request sees live kernel-owned
  memory, not a stale client frame. The slot is held for the life of the server, which is the
  documented limitation for a long-lived server with repeated cancellations (S36 can add the
  kill/cancel handoff that retires the request at `SEND`).
* Server death: `on_process_exit` fails every queued and delivered request with `EIO`. The
  client wakes; writable captures are copied back if the client is still mapped. The server's
  page tables are released afterwards; the capture window is skipped by the generic teardown
  and the capture objects free their own frames.

After unmapping a slot the kernel calls `sched::smp::shootdown::flush_range`. If the shootdown
does not complete (timeout), the frames are **not** returned to the allocator; the capture
counter shows them as leaked rather than risking a stale translation to recycled memory.

## 6. Accounting

`capture::frames_in_use()` is an atomic count of kernel-owned capture frames. With the
`s32-capturecheck` boot token, `ipc::on_process_exit` prints

```
s32: capture frames=<N> free_bytes=<M>
```

after the IPC drains. `N` must be zero at every fixture process exit; the qtest forbids a
non-zero value. The `free_bytes` field is the existing frame-allocator probe.

## 7. Known limitation

The current process ABI has no per-request cancel and no kill; `EXIT` from one thread of a
multi-threaded process does not terminate the other threads until the last one exits. A client
blocked in a two-way capture request therefore cannot exit, so the fixture's cancellation case
exercises the process-exit path with a queued one-way request. The retention logic for a
delivered capture is implemented for the future kill/cancel path (S36) and is exercised by the
server-death case (the server side releases the capture while the client is still alive). A
kill-based regression should be added when S36 lands.
