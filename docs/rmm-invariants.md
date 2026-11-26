<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-rmm invariants and ownership (RM1)

This is the written A-grade contract for the frozen page-table mapper described in
`docs/redox-retirement.md` (S117, DR-63). It documents the invariants the mapper enforces, the
ownership rules around the frame allocator and the direct map, the unsafe blocks, and the
limits that remain outside the mapper. The host tests are
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
| Table ownership and free | frames come only from `FrameAllocator` and are zeroed before use; new tables are linked only after the leaf is set; a failed map frees unlinked tables; unmap frees a child only after the parent entry is cleared; an unreadable table is not considered empty | `rmm_model::map_oom_frees_new_tables_and_clears_the_root`, `page_table_is_empty_is_conservative_when_unreadable`, `mapper::unmap_frees_empty_intermediate_tables` |
| TLB coherence | every visible leaf change (`map`, `protect`, `unmap`) calls `TlbFlusher::flush`; pruning an empty child flushes after the parent change; the caller owns AS switches via `flush_all` | `rmm_model::tlb_flush_is_emitted_for_map_protect_unmap`; the `FakeTlb` in the mapper tests |
| COW | not implemented in the mapper; a pager must use write-protection plus a fault path | no mapper COW test; documented limit |
| MMIO | `PageFlags::mmio()`/`kernel_device()` select device attributes; the mapper does not classify physical ranges | `mapper::mmio_flags_are_uncached_and_non_executable`, `aarch64::mmio_selects_device_ngnre` |
| Partial `map_range` | a multi-page map is not rolled back on error; the contract is documented on the method | `map_range_*` tests; documented limit |
| aarch64 provenance | MIT/Jeremy Soller notice and the S124 `PROVENANCE.toml` entry for `rmm/src/arch/aarch64.rs` at `015af335...` | tree check in the RM1 report; `LICENSE-MIT-REDOX` retained |

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
4. `unmap` clears the leaf first, flushes, then prunes empty children. A child is freed only if
   its `is_empty` read succeeded and the parent entry was cleared. `PageTable::is_empty` returns
   false for an unreadable table, so a broken direct map cannot cause a live table to be freed.
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

All production `unsafe` is in three table/direct-map helpers. The invariants are:

| Location | Unsafe operation | Invariant and coverage |
|---|---|---|
| `page/table.rs::entry` | `read_volatile` of an 8-byte entry | `PhysMemory` returns a non-null, writable, 4 KiB direct-map pointer; `index < 512`; entry offset is 8-byte aligned. The mapper tests read every level; the model exercises random walks. |
| `page/table.rs::set_entry` | `write_volatile` of an 8-byte entry | same pointer/ownership invariant; the table frame is owned by this mapper (root by the caller). Covered by every map/unmap/protect test and the OOM/recycle model. |
| `page/table.rs::entry_pointer` | pointer arithmetic on the direct-map base | base points at a 4 KiB frame; `index * 8` stays inside it because `index < PAGE_ENTRIES`. Covered by all table walks. |
| `page/mapper.rs::zero_frame` | `write_bytes` of one 4 KiB frame | `PhysMemory` guarantees a writable 4 KiB frame; the frame came from `FrameAllocator` and is not linked yet. Covered by `map_oom_*` (zeroed frame freed on failure) and every successful map. |
| `page/mapper.rs::new_address_space` | two `read_volatile`/`write_volatile` copies of the upper 256 root entries | both pointers are direct-map pointers for 4 KiB roots; `index` is 256..512; the destination was freshly zeroed. Covered by `new_address_space_copies_the_upper_half` and the new root-failure tests. |

Test-only `unsafe` exists in the `FakePhys` arenas (`tests/mapper.rs`, `tests/aarch64.rs`,
`tests/rmm_model.rs`); it is not part of the production crate. `grep -rn "unsafe" crates/raven-rmm/src`
is the authoritative production list for the report.

## 6. aarch64 provenance

`crates/raven-rmm/src/arch/aarch64.rs` is an adapted Redox `rmm` aarch64 backend. The file keeps
the MIT/Jeremy Soller notice, and the S124 entry in `PROVENANCE.toml` records:

- upstream `redox-kernel`, path `rmm/src/arch/aarch64.rs`,
- revision `015af335c7bed8bfaa8c2ae8419ceba46fe0a227`,
- licence MIT, tier A, `modified = true`,
- the known Redox TODOs fixed here (separate PXN/UXN, 48-bit width, TTBR0/TTBR1 validity).

The removal of the Redox `Arch` trait and inline-asm bodies is recorded in the same entry. No
new Redox import was added in RM1. `LICENSE-MIT-REDOX` remains in the crate. The NetBSD
`sys/arch/aarch64/aarch64/pmap.c` design is the future replace-on-touch donor, not a copy
source in this ticket.

## 7. Measurements (OBSERVED)

The model prints `ops`, `p50_ns` and `p90_ns` per seed/base/encoding. The RM1 report records the
exact long-corpus output from `timeout 40 sh tests/rmm/run-model.sh`. The 2026-11-19 run used
50,000 operations per seed per virtual half per encoding: **600,000 modeled operations total**,
x86-64 max p90 **1,540 ns**, aarch64 max p90 **1,777 ns**, wall **1.188 s**. These are host model
latencies, not boot-time or release claims; the kernel/SMP numbers remain with S83/S118 and
S126-r1. The fixed corpus is three seeds x two virtual halves x two encodings.

## 8. Known limits and follow-ups

- 1 GiB mappings are translate-only; mapping one is deliberately rejected. A future RM2 that
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
| Does Illumos do it the same way? | `illumos-gate` `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` (CDDL, tier B): a per-AS HAT with platform page tables, pv/hati bookkeeping and explicit TLB flush. The worktree is not present in this clone, so this is design-only from the established S101/DR-52 notes; no files were copied. | Keep the existing RavenBSD mapper; document the TLB seam. No copied code. |
| Does BSD have an awesome implementation we can take as-is? | NetBSD `sys/arch/aarch64/aarch64/pmap.c` at `36eddebcc36cb0775aa2962005b9c05419a6ff01` (BSD-2-Clause): `pmap_create` allocates the L0 table and pv lists; `pmap_enter`/`pmap_remove`/`pmap_protect` walk L0..L3; `AARCH64_TLBI_BY_ASID_VA` flushes by ASID/VA. It is far more than a table mapper (pv/ASID/kernel growth), so it is the future replace-on-touch design, not an RM1 copy. | Do not copy in RM1; record the path and keep the MIT-adapted file. |
| How did Linux do it? | `linux` `adc218676eef25575469234709c2d87185ca223a` (GPL, tier C): per-mm page tables, `pte_alloc`/`pmd_alloc` ownership, `tlb_flush`/`flush_tlb_range` and a user/kernel split; the checkout in this workspace has only `arch/` and `drivers/`, so no `mm/` source was read. | Outcomes/ownership lessons only; no Linux code. |
| Outcome | A-grade evidence without a rewrite: written invariants, adversarial/OOM/recycle model, SMP/TLB case, unsafe audit, aarch64 provenance recorded. | `docs/rmm-invariants.md`, `tests/rmm/`, `tests/qtest/cases/rmm-*`, `crates/raven-rmm/tests/rmm_model.rs`. |
