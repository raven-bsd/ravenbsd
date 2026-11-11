<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel filesystem plumbing

Status: S37 implementation for the K66 contract (`docs/design/fs-contract.md`). The shared
protocol, `raven-scheme` and `raven-rt` sides are K66; this note covers the kernel half.

## 1. Identity on the wire

There are two ids:

| Id | Owner | Used for |
|---|---|---|
| **kernel file id** | `SchemeFile.id`, `next_file_id()` | the `OPEN` packet only; kernel bookkeeping |
| **server file id** | `server_cookie` in the `OPEN` response | every later `READ`/`WRITE`/`FSTAT`/`CALL`/`DUP`/`CLOSE` packet's `file_id` |

After a successful `OPEN`, `router` stores the response cookie and sends it as the wire
`file_id` (and also as `server_cookie`) for every later request. This is what the shared
`raven-scheme::Server` expects: its handler receives `request.file_id` and keys `OpenTable` by
it. Servers that predate K66 (ramfsd/initfsd) return the kernel id as their cookie, so their
state maps continue to work unchanged. A zero cookie falls back to the kernel id.

A `SchemeFile` handle is a transferable capability (`Rights::TRANSFER` is part of the open
rights). An attachment or `HANDLE_TRANSFER` clones the same kernel object; no `DUP` packet is
sent for a transfer, because the server's open-file description already exists.

## 2. Open flags

The kernel carries the eight K66 wire bits (`WIRE_OPEN_*`, mask `0xFF`) and rejects unknown
bits with `EINVAL`. Only the access bits select kernel handle rights: read/write (plus `DUP` and
`TRANSFER`); the descriptive bits (`CREATE`, `EXCL`, `TRUNC`, `APPEND`, `DIRECTORY`,
`NOFOLLOW`) travel to the server. The server's `OpenFlags::from_wire` performs the combination
checks and the atomic `O_EXCL`/`O_APPEND`/`O_TRUNC` operations; the kernel has no per-server
flag table.

## 3. Offsets

Protocol version 1 is sequential. The `READ`/`WRITE` syscalls have no offset argument and the
kernel always sets `SchemePacket.offset = 0`. The server's cursor lives in the open-file
description and is shared by `DUP`/transfer. A nonzero wire offset is rejected by the server
with `EINVAL` (K66); the kernel cannot produce one through the current ABI. There is no
positional read/write.

## 4. DUP and the last close

* `DUP` duplicates the kernel handle (same `SchemeFile` Arc) and sends a `Dup` request with the
  server file id. The submit is mandatory: if it fails, the duplicate handle is closed and the
  syscall returns the error, so the server never misses the reference increment for a live
  duplicate.
* `SchemeFile.server_refs` starts at one for a successful `OPEN` and grows by one per delivered
  `Dup`. `CLOSE` only closes the local handle; no packet is sent while another handle,
  attachment, transfer or in-flight request still refers to the kernel object.
* The explicit close that drops the last reference sends `server_refs` `Close` requests, which
  matches the server's `OpenTable` reference count (`1 + dups`). The requests are queued before
  the caller's next request, so a following operation observes the released state.
* If the last reference drops inside the server `SEND` path (an in-flight request was the last
  holder), the close notifications are queued in `CLOSE_NOTICES` and drained at the top of the
  next `RECV`, where the kernel stack is shallow. This avoids building 440-byte packets in the
  deep reply path.

`server_refs` is a count of *server* references, not kernel handles. A transfer or attachment
does not increment it.

## 5. Client death

When a client exits, `ipc::on_process_exit` drains its outstanding requests. `READ`/`WRITE`/
`OPEN`/`CALL`/`FSTAT` are cancelled with `EIO`; one-way `CLOSE` and `DUP` notifications are
**not** cancelled, because they are the client's last words and the server still needs them.
The process handle table is dropped after `on_process_exit`; the `SchemeFile` Arcs drop then.
If the dying process held the last reference, `SchemeFile::drop` queues the last-close
notifications; the server drains them on its next `RECV`. A transferred or attached reference
in another process keeps the open-file description alive, so the unlinked data is not released
early.

A server that dies with captures or requests installed is still handled by `on_process_exit`
(before its address space is released), as in S32/S33.

## 6. Tests

`tests/fixtures/s37-fs/` and `tests/qtest/cases/fs-ipc.toml` exercise:

* the K66 flag matrix: `O_EXCL` twice, `O_TRUNC`, `O_DIRECTORY` on a file and a directory,
  `O_NOFOLLOW`, unknown bits and invalid combinations;
* sequential reads and writes (two 4-byte reads match one 8-byte read; two writes append at the
  cursor), with the server asserting every wire offset was zero;
* `DUP` then close-one: the survivor keeps the shared cursor; the server's live open count
  returns to zero;
* 10,000 open/dup/close cycles: the live server open count, released count and offset-violation
  count all return to baseline;
* forged/closed kernel handles: `EBADF`, no panic;
* client death: a one-shot death server opens an unlinked file, returns it as an `Open`
  `REPLY_HANDLE` attachment and exits; the controller transfers the attachment into a
  separately loaded death reader, which reads `unlinked-data`, closes the last reference and
  exits. The server reports `released=1` and `opens=0`.

## 7. Known limits

* The server file-id counter (`OpenTable::next_id`) is monotonic; the live count returns to
  baseline, but ids are not reused. That is a count of live state, not a leak.
* The current `READ`/`WRITE` ABI has no offset argument; positional I/O is not available in
  protocol version 1. The server side is ready to reject a nonzero offset, and K66's fake
  harness tests that path.
* `Process` handles are not transferable, so a client cannot hand a process capability to a
  helper; the death test uses an `Open` attachment (a SchemeFile capability) and the controller
  transfers it into the reader it loaded. This is the C14 transfer path, not a process
  capability.
