<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Scheme-IPC message robustness (K469)

Status: **OBSERVED on base `49af62a3`, x86-64, TCG `smp=4` and KVM `smp=2`.** The guest fixture
`tests/smp/ipc-fuzz` registers a scheme in one process and serves it from the main thread; a client
thread opens the scheme and issues one `CALL` per fuzz group. The server replies with deterministic
malformed `SchemePacket`s and the fixture records both the server's `SEND` return and the client's
request result. It fixes nothing: the deviation is a finding for the IPC owner.

## Scope and method

* `SchemePacket` is the kernel's fixed 536-byte scheme record; the fixture uses the same
  `raven-ipc` codec as the kernel, so the wire image cannot drift.
* Seed `0x49504346555a5a00`; the malformed vectors are fixed by construction.
* Groups (15): truncation at every byte boundary, `data_len` 0/1/`u32::MAX`, opcode 0/99, a wrong
  valid opcode, a valid opcode with an oversized payload, a reply larger than the request's
  response capacity, `attach_count` 5, an out-of-range handle, a closed handle, a duplicated
  handle, `Rights(!0)`, and a mask with bits the sender does not hold.
* The server's `SEND` path (`kernel/src/ipc/router.rs::send`) is the untrusted-message receiver;
  the client's raw `CALL`/`READ`/`PREAD`/`SEEK` vectors exercise the request-construction side.
* A valid reply follows every malformed reply; after a reply that completes the request, the
  request handle is retired and the follow-up `SEND` is expected to be `EBADF`. The next group's
  `CALL` proves the channel is still usable, and a sentinel `CALL` ends the server loop.

## Commands and result (OBSERVED)

```text
$ timeout 300 sh tests/smp/ipc-fuzz/run.sh tcg --iterations 1
rows=587 deviations=1 seed=0x49504346555a5a00 table-hash=0x8f0a578de897110a
real 48.88 s (image rebuild included)

$ timeout 300 sh tests/smp/ipc-fuzz/run.sh kvm2 --iterations 1
rows=587 deviations=1 seed=0x49504346555a5a00 table-hash=0x8f0a578de897110a
real 24.11 s
```

One iteration is a determinism pair: two boots of the same image. Both boots in each mode printed
the same hash, and TCG and KVM produced the same hash. `ipc-fuzz: alive` and `init exited (0)` were
present in all four boots. Full logs: `build/ipc-fuzz/<mode>/run-1/a/ipc-fuzz*.log`.

## Message table

`SEND` is the server's parse result; `follow-up` is the valid reply after it; `client` is what the
peer's `CALL` returned. `EBADF` on the follow-up means the request was already completed and the
handle retired (documented).

| Group | Malformed reply | SEND | Follow-up | Client |
|---|---|---|---|---|
| truncation | lengths 0..535 of a valid packet (536 rows) | EINVAL | 0 | 0 (one call) |
| data-len-0 | `data_len=0` | 0 | EBADF | 0 |
| data-len-1 | `data_len=1` | 0 | EBADF | 0 |
| data-len-max | `data_len=u32::MAX` | EPROTO | 0 | 0 |
| opcode-0 | `opcode=0` | EPROTO | 0 | 0 |
| opcode-99 | `opcode=99` | EPROTO | 0 | 0 |
| wrong-opcode | `opcode=Read` on a Call request | EPROTO | 0 | 0 |
| wrong-opcode-len | `opcode=Open`, `data_len=257` | EPROTO | 0 | 0 |
| buffer-over | `buffer_len=65` with a 64-byte response capture | EPROTO | EBADF | EPROTO |
| attach-count-5 | `attach_count=5` | EPROTO | 0 | 0 |
| attach-range | `attach[0].handle=0x7fff_ffff` | EBADF | EBADF | EBADF |
| attach-closed | `attach[0].handle` is a closed handle | EBADF | EBADF | EBADF |
| attach-dup | the same memobj twice | 0 | EBADF | 0 |
| attach-rights | the memobj with `rights_mask=u32::MAX` | 0 | EBADF | 0 |
| attach-mask-extra | the memobj with mask `0x8000_0000` | 0 | EBADF | 0 |

Standalone client rows (the channel the client does not own):

| Vector | Result | Meaning |
|---|---|---|
| call-reqmax | E2BIG | a `u64::MAX` request length fails in the capture path |
| call-respmax | E2BIG | a `u64::MAX` response length fails in the capture path |
| read-lenmax | E2BIG | a `u64::MAX` read length fails in the capture path |
| pread-wrap | **0** | `offset=u64::MAX-7`, `len=16` is accepted and the server returns 0 (**deviation**) |
| seek-whence | EINVAL | an unknown whence is rejected |
| send-file | EACCES | `SEND` on a read-only scheme file lacks `WRITE` |
| send-forged | EBADF | `SEND` on a handle not in the table |

The client's `SEND` on its scheme file is the "channel it does not own" case: the handle is a
`SchemeFile` without the request right, so the kernel refuses with `EACCES`/`EBADF` before any
packet is parsed. The server-identity `EACCES` branch (`request.server != caller` with a valid
request handle in a foreign process) is listed as a coverage gap.

## Deviation

One row returned success for a wrapping length:

| Vector | Expected | Actual | Kernel path (OBSERVED) |
|---|---|---|---|
| PREAD `offset=u64::MAX-7`, `len=16` | EOVERFLOW/EINVAL/EFAULT | 0 (the server returns a zero-length read) | `kernel/src/syscall/ipc.rs:171` `pread` -> `kernel/src/ipc/router.rs:169` `pread` -> `:176` `read_at` -> `:197` `packet.offset = positional.unwrap_or(0)` (no `offset.checked_add(len)`) |

`write_at` has the same construction at `kernel/src/ipc/router.rs:273`; the fixture opens the file
read-only, so the `PWRITE` side is not measured (coverage gap). A permissive server that ignores
`offset` returns success; a strict server might reject it, but the kernel should reject the
overflow before the request leaves the process. Proposed follow-up (PROPOSED, not fixed here):
`read_at`/`write_at` return `EOVERFLOW` when `offset.checked_add(len)` is `None`, with a unit test
for both directions.

## Documented variations (OBSERVED)

* Huge client lengths (`u64::MAX`) return **E2BIG** from the capture allocator, not
  EINVAL/EFAULT. This is a safe resource/argument-size error (no success, no panic); the ticket's
  errno list did not include it.
* A malformed reply that `decode`/`validate` rejects (`EPROTO`) leaves the request pending, so the
  follow-up valid reply completes it and the peer sees success. A reply that fails the result-bound
  or attachment step calls `request.fail`, retires the handle, and the peer sees that error; the
  follow-up `SEND` then returns `EBADF` (the handle is closed).
* `EPIPE` was not observed: the scheme request path reports a dead peer as `EIO`/`EBADF`, not
  `EPIPE`; the channel stayed usable in every group.
* Attachment masks are reduction-only (`attachment_rights` intersects with the sender's rights), so
  `Rights(!0)` and foreign bits cannot escalate; both attach successfully with the intersected
  rights. A post-attachment capability-use test is a coverage gap.

## Coverage gaps

* The `EACCES` server-identity branch (a live request handle transferred to a foreign process whose
  `SEND` targets another server) is not reached; the handle-table rejection is covered.
* `PWRITE` wrapping offset is not measured (the fixture's file is read-only); the code path is the
  same `write_at` construction.
* The 536 truncation rows exercise the `SEND` `length < PACKET_SIZE` gate; `decode`'s `TooShort`
  branch is unreachable from `SEND` because the syscall copies exactly `PACKET_SIZE` bytes.
* Open/Read/Write parser replies are exercised only through the Open handshake; the malformed
  vectors target Call replies. A scheme server returning malformed Read/Write replies is a
  follow-up surface.
* The attached-handle rights are not exercised after delivery; only the install path is measured.

## Donor reconciliation

Test and documentation only; no kernel, libc or driver code is written. The donor tree and
`donors.lock` are absent in this clone. The expectations come from POSIX and the scheme contract;
the nearest normative files are named for the owning fix ticket.

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| EFAULT/EINVAL/EOVERFLOW for bad lengths | copyin/copyout range contract; no source in this clone | `sys/amd64/amd64/support.S` (`copyin`), `sys/vm/vm_fault.c` — BSD-2-Clause | `arch/x86/include/asm/uaccess.h` (`access_ok`), `fs/read_write.c` — GPL, read-only | expectation only |
| pread offset+len overflow | vnode offset contract | `sys/kern/vfs_syscalls.c` (`kern_preadv`) — BSD-2-Clause | `fs/read_write.c` (`ksys_pread64`) — GPL, read-only | expectation only; IPC owner fixes |
| EBADF/EACCES for request handles and rights | descriptor and rights contract | `sys/kern/kern_descrip.c` (`fget`), `sys/kern/uipc_syscalls.c` — BSD-2-Clause | `fs/file.c`, `net/socket.c` — GPL, read-only | expectation only |
| EPROTO for malformed wire messages | n/a | n/a | n/a | first-party `raven-ipc` wire format |

## K456/K462 cross-reference

The syscall surface was confirmed on this base after the K461 fix: `tests/smp/syscall-fuzz`
printed 432 rows, 0 deviations, hash `0x9d29973842b31b57`. That suite is not edited by K469.
