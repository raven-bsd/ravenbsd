<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-rmm invariants and ownership (RM2)

This is the written A-grade contract for the first-party `raven-rmm` page-table mapper. RM2
replaced the Redox-derived implementation with a BSD-3 implementation of the NetBSD pmap design
(S117 order 1-3, `docs/redox-retirement.md`, DR-63); see `tickets/mark/reports/RM2.md`. The
public API and the `PhysMemory`/`TlbFlusher` seams are unchanged. This document lists the
invariants the mapper enforces, the ownership rules around the frame allocator and the direct
map, the unsafe blocks, and the limits that remain outside the mapper. The host tests are
`crates/raven-rmm/tests/mapper.rs`, `crates/raven-rmm/tests/aarch64.rs` and the adversarial model
`crates/raven-rmm/tests/rmm_model.rs`; the long corpus wrapper is `tests/rmm/run-model.sh`.

Evidence tags: **OBSERVED** means read or measured in this tree; **PROPOSED** means a contract
that is enforced by code/tests but has no kernel integration evidence yet; **UNVERIFIED** means
not measured.

## 1. Invariant table

| Invariant | Enforcement | Test evidence |
|---|---|---|
| W^X: no writable+executable leaf | `Mapper::map`, `Mapper::protect`, `Mapper::map_range` return `MapError::WriteExecute` before changing a table | `mapper::write_execute_map_is_rejected`, `aarch64::writable_executable_is_rejected`, `rmm_model` flag corpus |
| User/kernel split: user mappings only in the user half | `Encoding::is_user_half`; `map`/`protect` return `MapError::UserKernelSplit` for a user flag in the kernel half | `rmm_model::user_mapping_outside_the_user_half_is_rejected` (x86 and aarch64) |
| Canonical virtual address | `E::is_canonical` before any walk; x86 48-bit rule, aarch64 TTBR0/TTBR1 48-bit halves | existing canonical tests; `VirtAddr` docs |
| Physical width | `PHYS_ADDR_MASK` on root and every allocated table/leaf; `MapError::NotCanonical` | `mapper::physical_width_is_checked`, `aarch64::physical_address_above_48_bits_is_rejected` |
| No double map | a present target entry returns `AlreadyMapped`; a present block/table shape mismatch returns `HugePageConflict` | `mapper::double_map_is_rejected`, `aarch64::double_map_is_rejected`, model |
| Huge/4K conflict, no silent split | 4 KiB over a 2 MiB/1 GiB leaf and 2 MiB over a 4 KiB table both return `HugePageConflict`; 1 GiB mapping is deliberately rejected | `mapper::map_4k_under_2m_conflicts`, `map_2m_over_4k_conflicts`, `aarch64::page_over_block_conflicts`, model 1 GiB branch |
| Table ownership and free | frames come only from `FrameAllocator` and are zeroed before use; new tables are linked only after the leaf is set; a failed map frees unlinked tables; unmap frees a child only after the parent entry is cleared; an unreadable table is not considered empty | `rmm_model::map_oom_frees_new_tables_and_clears_the_root`, `page_table_is_empty_is_conservative_when_unreadable`, `mapper::unmap_frees_empty_intermediate_tables`, `mapper::unmap_2m_frees_both_empty_intermediate_tables` |
| TLB coherence | every visible leaf change (`map`, `protect`, `unmap`) calls `TlbFlusher::flush`; pruning an empty child flushes after the parent change; the caller owns AS switches via `flush_all` | `rmm_model::tlb_flush_is_emitted_for_map_protect_unmap`; the `FakeTlb` in the mapper tests |
| COW | not implemented in the mapper; a pager must use write-protection plus a fault path | no mapper COW test; documented limit |
| MMIO | `PageFlags::mmio()`/`kernel_device()` select device attributes; the mapper does not classify physical ranges | `mapper::mmio_flags_are_uncached_and_non_executable`, `aarch64::mmio_selects_device_ngnre` |
| Partial `map_range` | a multi-page map is not rolled back on error; the contract is documented on the method | `map_range_*` tests; documented limit |
| Provenance | every `raven-rmm` source is first-party BSD-3; no Redox/MIT-derived file remains | source scan in the RM2 report; the eight `PROVENANCE.toml` removals are reported for K264 |

## 2. Mapping contract

`Mapper::map(virt, phys, flags, size)` accepts `PageSize::Size4K` and `PageSize::Size2M`; a
`PageSize::Size1G` map is rejected with `HugePageConflict` because the mapper has no 1 GiB
allocation/ownership path. `translate` still decodes a pre-existing 1 GiB leaf (the S06 tables
and the aarch64 block tests cover that shape). A 4 KiB map under a block and a block over a
4 KiB table are both rejected; the caller must unmap the old shape first. The mapper never
splits a huge page.

`VirtAddr::new` applies the x86-64 canonical rule. aarch64 callers that need the full TTBR0 or
TTBR1 48-bit ranges use `VirtAddr::new_raw` and the encoding's `is_canonical` check; the mapper
re-checks with its `Encoding`. `u64::MAX` is reserved as the crate's invalid sentinel and is
rejected by `new_raw`.

The user/kernel split is enforced by `Encoding::is_user_half`: x86-64 uses the lower canonical
half, aarch64 the TTBR0 half (top 16 bits zero). A user-accessible leaf or intermediate table in
the kernel half is refused. Kernel mappings in the lower half remain allowed for callers that
need identity/low mappings; the invariant is about user privilege, not about where the kernel
happens to map.

## 3. Table ownership and failure paths

The root frame is owned by the caller. Every other table is allocated by `FrameAllocator` and
freed only through `FrameAllocator::free`. The mapper's rules are:

1. A new table is zeroed before any entry is written. A zeroed table is a valid empty table.
2. A new intermediate table is not linked into its parent until the leaf entry has been written
   successfully. If an allocation, zero, or leaf write fails, all unlinked tables are freed and
   no root entry points at them. This closes the OOM leak where a failed 4 KiB map left one or
   two empty tables reachable from the root.
3. On a successful map, the new tables are linked top-down. If a link write fails on a broken
   direct map, the already linked entries are unlinked and the frames are freed only if every
   unlink succeeded; otherwise the frames are left allocated rather than freed under a live
   entry (safety over leak on an unreachable-in-practice failure).
4. `unmap` clears the leaf first, flushes, then prunes empty children. The RM2 prune walks the
   root once, records the parent entry of every child table, and frees from level 1 upward only
   after `is_empty` and a successful parent clear. The path includes the level-2/level-3
   parents of a 2 MiB leaf, so a block unmap also reclaims its empty PD/PDP tables.
5. `new_address_space` allocates and zeros a new root, then copies the upper half. If the direct
   map fails for either the source or the destination, the new root is freed before returning;
   the caller's source root is never modified.
6. A user address-space mapper must only unmap user addresses. Its kernel-half root entries are
   shared with the kernel root; pruning a shared upper-half table would free a kernel table. The
   crate documents this on `Mapper`; the kernel integration owns the policy.

The adversarial model asserts the end state: after unmapping every live mapping, the frame
allocator contains only the root frame. That is the table-recycle/OOM check.

## 4. TLB coherence

`map` flushes the virtual page after the leaf is linked. `protect` flushes after the descriptor
is rewritten. `unmap` flushes the leaf and flushes again when pruning frees a child. A caller
that switches address spaces or changes the root uses `flush_all`; the mapper's `TlbFlusher`
trait is the integration seam. The mapper does not issue cross-CPU shootdowns itself; the
kernel's shootdown implementation owns that protocol (S101/S102 context).

## 5. Unsafe audit

All production `unsafe` is in the table/direct-map helpers. The invariants are:

| Location | Unsafe operation | Invariant and coverage |
|---|---|---|
| `page/table.rs::entry` | `read_volatile` of an 8-byte entry | `PhysMemory` returns a non-null, writable, 4 KiB direct-map pointer; `index < 512`; entry offset is 8-byte aligned. The mapper tests read every level; the model exercises random walks. |
| `page/table.rs::set_entry` | `write_volatile` of an 8-byte entry | same pointer/ownership invariant; the table frame is owned by this mapper (root by the caller). Covered by every map/unmap/protect test and the OOM/recycle model. |
| `page/table.rs::slot` | pointer arithmetic on the direct-map base | base points at a 4 KiB frame; `index * 8` stays inside it because `index < PAGE_ENTRIES`. Covered by all table walks. |
| `page/mapper.rs::zero_frame` | `write_bytes` of one 4 KiB frame | `PhysMemory` guarantees a writable 4 KiB frame; the frame came from `FrameAllocator` and is not linked yet. Covered by `map_oom_*` (zeroed frame freed on failure) and every successful map. |
| `page/mapper.rs::new_address_space` | two `read_volatile`/`write_volatile` copies of the upper 256 root entries | both pointers are direct-map pointers for 4 KiB roots; `index` is 256..512; the destination was freshly zeroed. Covered by `new_address_space_copies_the_upper_half` and the new root-failure tests. |

Test-only `unsafe` exists in the `FakePhys` arenas (`tests/mapper.rs`, `tests/aarch64.rs`,
`tests/rmm_model.rs`); it is not part of the production crate. `grep -rn "unsafe" crates/raven-rmm/src`
is the authoritative production list for the report.

## 6. Provenance after RM2

All `crates/raven-rmm` source files are now first-party BSD-3-Clause with the RavenBSD header.
`crates/raven-rmm/LICENSE-MIT-REDOX` was deleted. The eight Redox `redox-kernel` imports in
`PROVENANCE.toml` are the only remaining records and must be removed by K264 (the provenance
owner): `src/lib.rs`, `src/arch/mod.rs`, `src/arch/x86_64.rs`, `src/arch/aarch64.rs`,
`src/page/mod.rs`, `src/page/entry.rs`, `src/page/flags.rs`, `src/page/table.rs`,
`src/page/mapper.rs` (the file list in `PROVENANCE.toml` has eight entries; `page/encoding.rs`
was already first-party). The NetBSD files below are the design source, not copied code:

- `sys/arch/x86/x86/pmap.c`, `sys/arch/x86/include/pmap.h`, `pmap_private.h`, `x86_tlb.c`;
- `sys/arch/aarch64/aarch64/pmap.c`, `sys/arch/aarch64/include/pmap.h`.

## 7. Measurements (OBSERVED)

The model prints `ops`, `p50_ns` and `p90_ns` per seed/base/encoding. The RM2 run used 50,000
operations per seed per virtual half per encoding: **600,000 modeled operations total**,
x86-64 max p90 **648 ns**, aarch64 max p90 **662 ns**, wall **0.2 s** (RM1 baseline: 1,540 ns
and 1,777 ns, so no regression). These are host model latencies, not boot-time or release
claims; the kernel/SMP numbers remain with S83/S118 and S126-r1. The fixed corpus is three seeds
x two virtual halves x two encodings.

## 8. Known limits and follow-ups

- 1 GiB mappings are translate-only; mapping one is deliberately rejected. A future ticket that
  needs 1 GiB ownership must add the matching allocation/free/split rules, not just a leaf.
- `map_range` does not roll back pages it already mapped when a later page fails. Callers must
  unmap the prefix; the method documents this. This is a contract limit, not a table leak.
- COW is a pager policy. The mapper provides write-protect and the flush seam; it has no COW
  refcount or fault path.
- The mapper does not classify physical ranges as RAM vs device; `MapError::NotDeviceMemory` is
  reserved for a caller/range policy that does not exist here. `PageFlags::mmio()` is the
  device-attribute path.
- The mapper has no ASID/PCID or cross-CPU shootdown policy; `TlbFlusher` is the seam and the
  kernel owns the protocol.
- `VirtAddr::new` is x86 canonicality; aarch64 TTBR0 addresses in the x86 hole need
  `new_raw` plus the encoding check. This is intentional API history.

## 9. Donor reconciliation (DR-52)

| Question | Finding | Decision |
|---|---|---|
| Does Illumos do it the same way? | `illumos-gate` `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (CDDL, tier B): a per-AS HAT with platform page tables, pv/hati bookkeeping and explicit TLB flush. The worktree is not present in this clone, so this is design-only; no files were copied. | No Illumos code; the TLB seam is unchanged. |
| Does BSD have an awesome implementation we can take as-is? | NetBSD `sys/arch/x86/x86/pmap.c` and `sys/arch/aarch64/aarch64/pmap.c` at `36eddebcc36cb0775aa2962005b9c05419a6ff01` (BSD-2-Clause): `pmap_enter`/`_pmap_enter`, `pmap_remove`, `pmap_protect`, `pmap_extract`, `pmap_create`, `pmap_destroy`, TLB shootdown. They are full pmaps (pv/ASID/kernel growth), more than a table mapper. | RM2 implements the same design in first-party Rust; no code copied. |
| How did Linux do it? | `linux` `adc218676eef25575469234709c2d87185ca223a` (GPL, tier C): per-mm page tables, `pte_alloc`/`pmd_alloc` ownership, `tlb_flush`/`flush_tlb_range` and a user/kernel split; the local checkout has only `arch/` and `drivers/`, so no `mm/` source was read. | Outcomes/ownership lessons only; no Linux code. |
| Outcome | RM2 replacement: first-party BSD-3 mapper, same public API, RM1 model/OOM/recycle tests plus the 2M prune test, SMP/TLB qtests, check-fast. | `docs/rmm-invariants.md`, `tests/rmm/`, `tests/qtest/cases/rmm-*`, `crates/raven-rmm/tests/rmm_model.rs`. |
