<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Memory contract rows (K453)

The `tests/smp/memory` fixture now runs seven memory-contract rows after the S85b TLB/allocator
stress. Each row prints `CONTRACT K1..K7 PASS|FAIL`, the fixture fails if any row is red, and the
existing `smp-memory: ok`/`summary` and TLB `forbid` checks remain unchanged. The frame table uses
the kernel's existing `s24-framecheck` evidence line after every child exit.

The rows are POSIX memory semantics with NetBSD uvm/sys_generic as the behaviour donor and FreeBSD
`vm_fault` as the oracle. No donor code is copied; the tests are original Rust.

## Rows

| Row | Contract | Expected on the current tree | Donor |
|---|---|---|---|
| K1 | Fault on an unmapped page | child killed with status `-11` (SIGSEGV); parent and workers alive; no panic | NetBSD `sys/uvm/uvm_fault.c` `uvm_fault_internal`/`UVM_FAULT_*` |
| K2 | Protection fault | write through `PROT_READ` and read through `PROT_NONE` both kill with `-11`; the serial reports the faulting child address | NetBSD `uvm_fault.c`; FreeBSD `sys/vm/vm_fault.c` oracle |
| K3 | COW across fork | parent's `0x11` bytes unchanged after child writes `0x22`; child exits 0; free-frame count returns to the baseline | NetBSD `sys/uvm/uvm_map.c` `uvm_map_pageable`, `uvm_fault.c` COW path |
| K4 | mmap/munmap/mprotect errno | `MAP_FIXED` collision `EEXIST` (17); `munmap` unmapped `EINVAL` (22); no-free-range `ENOMEM` (12); `mprotect` unmapped `ENOMEM` (12); access after `munmap` kills with `-11` | POSIX `mmap`/`munmap`/`mprotect`; NetBSD `sys/uvm/uvm_mmap.c` |
| K5 | OOM | 1 GiB `MEMOBJ_CREATE` fails with `ENOMEM`; a 1-page object succeeds; the next user allocation succeeds; frame count returns to the documented reserve | NetBSD `sys/uvm/uvm_page.c` `uvm_pagealloc_strat`; FreeBSD `vm_page` oracle |
| K6 | Stack growth/guard | 200 KiB below the current SP writes and reads; 300 KiB below faults with `-11` | POSIX stack limits; NetBSD `sys/uvm/uvm_map.c` stack growth |
| K7 | Usercopy EFAULT | `DEBUG_WRITE` at pointer `0x1`, past `USER_END`, and a wrapping length returns `EFAULT` (14); `UNAME`/`CLOCK_GET` at pointer `0x1` also return `EFAULT`; a valid `PROT_NONE` page returns `EFAULT` from `DEBUG_WRITE`, pipe `WRITE`, `UNAME` and `CLOCK_GET`; the process survives | NetBSD `sys/kern/sys_generic.c` `copyin`/`copyout`; FreeBSD `vm_fault.c` oracle |

The base fixture image has no scheme server, so `READ`/`FSTAT` file-descriptor handles are not
reachable; K7 uses the write-class `DEBUG_WRITE` and pipe `WRITE`, the read-out/stat-class
`UNAME`/`CLOCK_GET`, and a direct `PROT_NONE` page. The file-descriptor usercopy path remains
covered by the scheme qtests.

## x86-64 user-copy protection (K461)

`PROT_NONE` is encoded as a **present, supervisor-only, read-only** leaf
(`kernel/src/memory/uspace.rs` `none_leaf_flags`, `PRESENT | NO_EXECUTE`) so that `UNMAP` and
`PROTECT` can find and reuse the frame. Ring-3 reads fault on the clear U/S bit, but CPL0
`rep movsb` in `copy_from_user` ignores U/S on reads; before K461 the kernel could read a
`PROT_NONE` buffer and the syscall succeeded.

The fix keeps the present encoding and adds a software leaf check in x86-64 `copy_from_user`:

- the active CR3 root is walked once per 4 KiB page in the source range;
- a **present** leaf with the user bit clear returns `EFAULT` before the copy;
- a **present user** leaf uses the existing `rep movsb` fast path;
- a not-present leaf is left to the existing fault/fixup/VM path, so demand-paged copies are not
  rejected;
- `copy_to_user` is unchanged: a user write to the supervisor-only leaf faults in hardware;
- aarch64 is unchanged: `ldtrb`/`sttrb` enforce EL0 permissions.

This adds one page-table walk per page in a `copy_from_user` call (the asm copy is unchanged and
`copy_to_user` has no extra walk). Raven has no SMAP; Linux relies on SMAP to make supervisor
pages inaccessible to kernel `copy_from_user`. That is a recorded divergence, and is why the
check is a software leaf walk. The `PROT_NONE` leaf was **not** made non-present, so
`mprotect`/COW/fork transitions keep their present-PTE semantics; the K453 rows (including K2
and K3) pass.

K7 now also covers a valid `PROT_NONE` page directly:

- `DEBUG_WRITE(prot_none, 1)` and `WRITE(pipe, prot_none, 1)` return `EFAULT` (14);
- `UNAME`/`CLOCK_GET` into the same page return `EFAULT`, confirming `copy_to_user` is unchanged.

### aarch64 measurement (K467)

The aarch64 guest suite (`tests/smp/aarch64`, case `smp-tlb-aarch64-concurrent`) is the aarch64
measurement; the `tests/smp/memory` case is x86_64-only (`build.sh` exits 77 for aarch64), so it
cannot be the aarch64 run. The aarch64 fixture now runs the same class:

- `DEBUG_WRITE` and pipe `WRITE` from a full `PROT_NONE` page and from a page straddling a valid
  page and a `PROT_NONE` page return 14;
- `UNAME`/`CLOCK_GET` into the `PROT_NONE` page return 14 (`copy_to_user`);
- the next valid `DEBUG_WRITE`/`UNAME`/`WRITE` succeeds and the process stays alive.

Measured on `smp-tlb-aarch64-concurrent`: all six values `Some(14)`, `next-debug/uname/write=true`,
`smp-aarch64-copy: ok`, `smp-aarch64-tlb: ok`, `tlb_stale=0`. The x86-64 equivalent is the K456
fuzz (432 rows, `deviations=0`, identical determinism pair) plus K7 7/7. The aarch64 path is
`ldtrb`/`sttrb`; the mutation `ldtrb` -> `ldrb`, `sttrb` -> `strb` makes the aarch64 row read the
supervisor leaf (`protnone-debug=None straddle-debug=None ...`) and fail, proving the measurement
is not tautological. The check is restored; no kernel change remains.

## Frame accounting

`s24: frames free=` is printed by the kernel after every child exit (the existing
`s24-framecheck` boot flag). The values below are bytes; the baseline is the last existing
fork/exit child before the contract rows.

| Run | Baseline | After COW (K3) | After OOM #1 | After OOM #2 | After stack fault (K6) |
|---|---:|---:|---:|---:|---:|
| TCG 4 | 506,363,904 | 506,363,904 | 505,311,232 | 505,278,464 | 505,311,232 |
| KVM 2 | 506,335,232 | 506,335,232 | 505,282,560 | 505,282,560 | 505,282,560 |
| KVM 4 | 506,265,600 | 506,265,600 | 505,212,928 | 505,212,928 | 505,212,928 |

The COW row returns to the baseline exactly. The first OOM attempt leaves a **1,052,672-byte
(257-frame) kernel heap/bookkeeping reserve**; the second identical OOM attempt does not grow it,
so it is a one-time reserve, not a per-attempt leak. The TCG second line is a 32 KiB transient
that returns to the first-OOM line at K6; the KVM runs show the stable value directly. The ticket
allows a documented cache reserve; that reserve is recorded here. A real frame leak would show a
further 1 MiB-scale drop on the second OOM child.

## Commands and result

```sh
timeout 600 sh tests/smp/memory/run.sh tcg --iterations 1   # PASS, all 7 rows, 140.7 s
timeout 600 sh tests/smp/memory/run.sh kvm2 --iterations 1  # PASS, all 7 rows, 7.6 s
timeout 600 sh tests/smp/memory/run.sh kvm4 --iterations 1  # PASS, all 7 rows, 6.6 s
```

The existing TLB lines remain: `smp-memory: summary alloc_ops=256 fault_pages=512 maps=32
forks=4 tlb_rounds=64 tlb_stale=0`, no `smp: tlb shootdown timeout`, and `smp-memory: ok`.

## Status of the "red first" rule

On `b2b89219` the six kernel-behaviour rows are already green; no memory-owned defect was found,
so no kernel fix was made. The only red row in the first run was K7's `FSTAT` path, which failed
because the base fixture image has no scheme server (`open("initfs:/init")` -> ENOENT), not because
of a kernel memory defect; the row was corrected to the reachable `UNAME`/`CLOCK_GET` read-out
class. No assertion was relaxed: K7 still requires `EFAULT` from bad pointers. The negative path of
every row is built in: a fault child exits 1 if the access unexpectedly succeeds, and the errno
rows compare the exact value.
