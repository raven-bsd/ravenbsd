<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Memory contract rows (K453/K475)

The `tests/smp/memory` fixture now runs seven memory-contract rows after the S85b TLB/allocator
stress. K475 adds the frame-exhaustion OOM injection row described below. Each row prints
`CONTRACT K1..K7 PASS|FAIL`, the fixture fails if any row is red, and the existing
`smp-memory: ok`/`summary` and TLB `forbid` checks remain unchanged. The frame table uses the
kernel's existing `s24-framecheck` evidence line after every child exit.

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

## Frame-exhaustion OOM injection (K475)

K5 exercises the object layer: a 1 GiB `MEMOBJ_CREATE` is refused before a frame is touched. K475
adds the frame-allocator row after K7. Each round starts **two children concurrently** through a
shared `memobj` barrier page, then:

1. each child maps a 16-page first chunk and then 1 MiB (`256`-page) chunks until the allocator
   refuses three consecutive times; every refusal must be `ENOMEM` (12), and the chunk list is
   bounded at 640 chunks;
2. every mapped page is filled with a distinct 64-bit pattern derived from `(seed, chunk, page)`,
   so a frame handed out twice is overwritten and detected;
3. both children meet a second barrier while still holding every frame, then read every page back
   and fail on the first mismatch (`oom: fail double-handout ...`);
4. both children unmap every chunk and print `oom: pass refusals=N frames_back=<bytes>`; the parent
   reaps both, allocates one page, writes/reads a marker, and prints `oom: parent-ok next=4096`;
5. the sequence runs twice (`oom: round=1`, `oom: round=2`) so a per-attempt leak would show as a
   further drop in the second post-round line.

The numeric baseline row is checked by `tests/smp/memory/run.sh`, because `s24: frames free=` is
kernel serial output that the in-guest fixture cannot read. The runner takes the last `s24` line
before `oom: round=1` as the baseline and the last `s24` line before each `oom: parent-ok` as the
post-round value. A reclaim leak fails the run; the second post-round value must also stay within
the documented 32 KiB cache transient of the first.

| Run | Baseline | Round 1 post | Round 2 post | Delta | Refusals | `frames_back` round 1 / round 2 (child A / child B) |
|---|---:|---:|---:|---:|---:|---|
| TCG `-smp 4` | 505,294,848 | 505,294,848 | 505,294,848 | 0 | 3 / 3 | R1 240,189,440 / 263,258,112; R2 222,363,648 / 281,083,904 |
| KVM `-smp 2` | 505,270,272 | 505,266,176 | 505,266,176 | -4,096 | 3 / 3 | R1 252,772,352 / 250,675,200; R2 235,995,136 / 267,452,416 |
| KVM `-smp 4` | 505,196,544 | 505,192,448 | 505,192,448 | -4,096 | 3 / 3 | R1 250,675,200 / 252,772,352; R2 262,209,536 / 241,238,016 |

Each round's two children release about 503 MiB together, which is the baseline minus the kernel
image, page tables and the parent's live mappings. The **4,096-byte difference is the test-owned
shared barrier page**, allocated after the K6 `s24` line: the delta is `-4,096` when that line
prints before the allocation and `0` when the delayed line prints after it. The parent unmaps the
page after both rounds. `post1 == post2` in both modes: the second attempt adds no reserve and
leaks nothing.

### Reserve arithmetic and the per-CPU page cache

The frame layer (`kernel/src/memory/frame.rs`) reports `free_bytes` as the sum of three terms:

```text
free_bytes = buddy.free + sum(cpu.cache.len) + sum(cpu.cache.remote_len)   [in frames]
```

The buddy manages `MAX_BLOCKS = 1 << 17` order-0 frames, i.e. 131,072 frames = **512 MiB**. The
per-CPU magazine type is sized for `NCPUS = MAX_CPUS = 64`; each CPU holds at most
`CACHE_CAP = 64` frames and each remote-free queue at most `REMOTE_CAP = 32`, so the full storage
bound is `64 * (64 + 32) = 6,144` frames = **25,165,824 bytes**. Those frames still count as free.
On the `-smp 4` runs only four CPUs allocate, and the measured drifts are far below the bound.
The low/high watermarks are 16/48 and the refill/drain batch is 32; a refill that hits an empty
buddy surfaces `OutOfFrames` and the caller returns `ENOMEM` rather than losing a frame.

The K453 one-time reserve is **1,052,672 bytes = 257 frames**, paid by K5 before the K475 baseline;
it is therefore already inside the baseline and is not charged again. The runner allows the
documented reserve, the 4,096-byte barrier page and the documented 32 KiB TCG transient, i.e.
1,089,536 bytes. A real reclaim leak is far larger (the mutation below loses 42,295,296 bytes) and
fails the row.

### Mutation/negative checks

- **Double handout.** Temporarily changed `kernel/src/memory/uspace.rs` `map_anonymous` so a
  failed `frame::alloc_zeroed_frame()` reuses the last mapped user frame for a bounded 1 MiB chunk
  (diff SHA-256 `848229142cf9817e57157acdf49bfda7044684dc7681db76a81c55e9ac76354a`). Both children
  failed the pattern check:
  `oom: fail double-handout chunk=208 page=0 seen=0xa5a400000d0000ff want=0xa5a400000d000000`
  and the same for chunk 274; the qtest forbade the line and exited 35. Reverted with
  `git restore kernel/src/memory/uspace.rs`.
- **Reclaim leak.** Temporarily changed `kernel/src/memory/frame.rs` `free_frame` to drop every
  eighth reclaimed frame (diff SHA-256
  `1fa58439cd991a7a94c9903028b1687551fd13b777b8aa8297da23022c020f44`). The qtest passed, but the
  runner failed the numeric row:
  `smp-memory: FAIL oom-baseline ... baseline=338071552 post1=295776256 delta=42295296 allowed=1089536`.
  Reverted with `git restore kernel/src/memory/frame.rs`.

No allocator defect was found on the production path: every refusal was `ENOMEM`, the guest never
panicked or printed `EXCEPTION`, no page pattern was overwritten, and both post-round lines returned
to the baseline. No permanent kernel change remains.

## Commands and result

```sh
timeout 600 sh tests/smp/memory/run.sh tcg --iterations 1   # PASS, 7 rows + 2 OOM rounds, 237.9 s (host load 44)
timeout 600 sh tests/smp/memory/run.sh kvm2 --iterations 1  # PASS, 7 rows + 2 OOM rounds, 29.1 s
timeout 600 sh tests/smp/memory/run.sh kvm4 --iterations 1  # PASS, 7 rows + 2 OOM rounds, 32.7 s
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
