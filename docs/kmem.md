# kmem: kernel allocator design

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED for A1–A5c. `raven-vmem` (A1), `raven-kmem` (A2), its per-CPU magazines/depot (A3),
the kernel heap integration (A4), the buddy core (A5a), the per-CPU frame caches (A5b) and the
kernel frame integration (A5c) are implemented and booting; A6 remains.

This is the working note for the memory lane. The requirement source is
`ravenbsd-prjmgmt/architecture/kernel-next/03-memory.md` (R-MEM-1 .. R-MEM-25); the allocator lane
backlog is `ravenbsd-prjmgmt/tickets/mark/backlog/BACKLOG.md`.

## 1. Layering

```
kmem (A2..A6): object caches -> magazines -> depot -> slabs
  ^
vmem (A1): arena -> span -> segment, the resource range allocator underneath
```

`raven-vmem` (`crates/raven-vmem`) is the pure, host-tested core that `raven-kmem` will sit on. It
manages any caller-supplied integer resource range — kernel virtual address space today, and later
frame/ID/handle spaces — so a single implementation backs both the heap and the ID arenas.

## 2. vmem model (A1, OBSERVED)

A `Vmem` arena owns a caller-supplied set of address ranges. Its state is a fixed-capacity slab of
**boundary tags** (one per segment), kept sorted by address; a segment is either free or allocated.
The slab is allocated once, in `Vmem::new` / `Vmem::with_capacity`, so `alloc`, `alloc_at`, `free`
and `import` never allocate. That is the bootstrap property that lets the arena itself back the
kernel allocator: an arena request cannot recurse into the very allocator it is serving.

Design choices, in our words, following the published vmem idea without copying any donor:

* A segment's length plus its base locates both physical neighbours, so neighbour coalescing is a
  constant-time merge and a binary search finds any base. The tags are plain `Vec` records rather
  than intrusive pointers because safe Rust forbids the pointer links; the sorted slab is the same
  boundary-tag structure with the same asymptotic cost.
* Every operation validates before it mutates. A request that would need a tag the slab cannot hold
  fails with `NoTags` and leaves the arena unchanged; a failed request is transactional.
* `free` requires the exact base and length of an allocated segment. Freeing a sub-range, a gap or a
  wrong length is an error, so a caller bug cannot silently corrupt the arena.

### Operations

| Operation | Meaning | Errors |
|---|---|---|
| `new(start, size)` / `with_capacity(..)` | create an arena over `[start, start+size)` | `ZeroLength`, `Overflow`, `Capacity`, `NoTags` |
| `import(base, size)` | add a disjoint range; coalesces with an adjacent free span | `ZeroLength`, `Overflow`, `Overlap`, `NoTags` |
| `alloc(size, align)` | allocate aligned, best-fit by default | `ZeroLength`, `Alignment`, `Exhausted`, `NoTags` |
| `alloc_with(size, align, policy)` | allocate with `FirstFit` / `BestFit` / `NextFit` | same as `alloc` |
| `alloc_at(addr, size, align)` | allocate exactly at an aligned address | `ZeroLength`, `Alignment`, `Overflow`, `OutOfRange`, `Overlap`, `NoTags` |
| `free(base, size)` | return an exact allocated segment; coalesces | `ZeroLength`, `UnknownSegment`, `DoubleFree`, `SizeMismatch` |
| `contains(addr)` / `contains_range(..)` | membership of the imported set | — |

Queries: `span_count`, `free_span_count`, `allocated_span_count`, `tag_capacity`, `largest_free`,
`total_free`, `total_allocated`, `fragmentation()` (`1 - largest_free / total_free`), `stats()`, and
the `segments()` / `allocated_segments()` / `free_segments()` iterators. `kmem` uses
`allocated_segments()` to name backing ranges and the kernel uses them to map/unmap.

### Invariants (enforced by the exhaustive and property tests)

1. Tags are sorted and non-overlapping; their union is exactly the imported address set.
2. No two adjacent tags are both free (neighbours coalesce on every free and import).
3. An allocated tag covers exactly the range the caller received.
4. Byte conservation: `total_free + total_allocated` equals the imported measure.

## 3. kmem model (A2, OBSERVED)

`raven-kmem` (`crates/raven-kmem`) is the pure `no_std` + `alloc` layer over `raven-vmem`. It adds
the general size classes, per-class slab free lists, named object caches and a `GlobalAlloc` shim —
still no kernel dependency and no global or static state: the caller builds the arena and hands it
to `Kmem::new`, which owns it.

* **General classes** (R-MEM-8): `8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024,
  1536, 2048, 3072, 4096, 8192`. `alloc(size, align)` routes a request to the smallest class that
  fits, or straight to the arena when it is above 8192 bytes or needs an alignment stricter than
  its class offers (`align > 16`). `free` uses the same `(size, align)`, so the pair always agrees.
* **Slabs**: a slab is one arena allocation cut into equal slots of `stride = align_up(size, align)`
  bytes; each slab keeps its free slots in a plain `Vec` of indices, so allocation is a pop and free
  is a push — no intrusive links in object memory and no `unsafe`. Fully-free slabs return to the
  arena through `reap` (and `cache_destroy`).
* **Object caches**: `cache_create(name, size, align)` / `cache_alloc` / `cache_free` /
  `cache_reap` / `cache_destroy` / `cache_stats`, addressed by a stable `CacheId`. The first
  [`SIZE_CLASSES`] slots are the general classes and cannot be destroyed.
* **`GlobalAlloc` shim** (`KmemAlloc`): `alloc`/`dealloc`/`realloc` over the caller-provided
  arena, with `realloc` staying in place across a size-class boundary and copying otherwise. This
  module is the only `unsafe` in the crate; the safe modules carry `#![forbid(unsafe_code)]`.
* **Alignment and statistics**: returned addresses are aligned to `align`; `CacheStats`/`KmemStats`
  expose allocs, frees, alloc failures, slabs, in-use/free objects, waste bytes and reaped slabs.

Typed caches (`Cache<T>` -> `Box<T, CacheAlloc>`, R-MEM-5) are not here: reconstructing a typed
pointer from an address is unsafe and belongs with the kernel integration (A4).

## 4. Magazines and depot (A3, OBSERVED)

`MagCache` adds the per-CPU magazine layer over the slab [`Cache`], with a [`Cpu`] trait supplying
the current CPU id and online count (the kernel reads these from hardware; host tests use a fake).
This is what removes the shared lock from the fast path.

* **Fast path**: [`MagCache::alloc`]/[`MagCache::free`] pop/push the current CPU's `loaded` magazine
  (falling back to `previous`); a successful operation touches only per-CPU data, takes no lock and
  performs no atomic read-modify-write. Magazines are preallocated to their capacity, so the fast
  path never allocates.
* **Depot** (slow path): when `loaded` and `previous` are both empty (alloc) or both full (free),
  the cache exchanges a whole magazine with the shared depot; only when the depot cannot help does
  it touch the slab layer.
* **Remote free**: a cross-CPU free lands in the freeing CPU's bounded remote queue (`rounds` slots);
  when it fills, the queue moves into a full magazine in the depot, and the allocator drains it on
  the next slow path. The depot itself is bounded to `2 * online` full magazines, draining the
  excess back to the slab layer.
* **Dynamic sizing**: `magazine_rounds(online)` scales the magazine from [`ROUNDS_MIN`] (15, one
  CPU) up to [`ROUNDS_MAX`] (143) with the online CPU count; the contention-driven doubling of
  R-MEM-7 stays deferred to A4, which owns the real per-CPU preemption context.
* **Correctness invariant** (tested): `slab_in_use == live + free_in_mags + free_in_depot +
  free_in_remote` at every step; the hammers drive 1/2/4 simulated CPUs and a million operations
  and assert no object is lost or duplicated.

## 5. Kernel heap integration (A4, OBSERVED)

`kernel/src/memory/heap.rs` now routes the global allocator through `raven-kmem`: a `KmemGeneric`
over a `VmemGeneric` arena spanning `KERNEL_HEAP_BASE`, with the initial 4 MiB mapped from fresh
frames and 1 MiB growth steps imported into the arena on allocation failure.

* **No recursion**: the arena's boundary tags and every slab/magazine freelist are fixed inline
  storage (reworked in A4 round 1), so allocation never re-enters the global allocator. `Vmem` is
  now `VmemGeneric<N>` (const tag capacity), `Cache` is `Cache<MAX_SLABS>` with a fixed free bitmap,
  `Kmem` is `KmemGeneric<MAX_SLABS, TAGS>` and `MagCache` is `MagCache<MAX_SLABS, NCPUS, NDEPOT>`;
  the `Arena` trait decouples the slab layer from the arena's tag capacity.
* **Boot stack**: the fixed allocator state (~900 KiB for `MAX_SLABS=256`, `TAGS=8192`) is
  constructed during `heap::init`, so the boot stack was widened from 64 KiB to 8 MiB (BSS) on both
  architectures.
* **Limits**: `TAGS=8192` bounds the arena to roughly 16 MiB of heap; `MAX_SLABS=256` bounds each
  size class to ~1 MiB of live objects. The OOM path is unchanged: `alloc` returns null and the
  runtime raises its allocation-error panic.
* The per-CPU magazine fast path is preserved and host-tested but is not yet wired into the kernel
  heap (the slab layer is the A4 fast path); that is the remaining piece of the original "use the
  boot CPU id and online count" note and lands with the magazine-kernel glue.

## 6. Debug modes and the hammer (A6, OBSERVED)

`raven-kmem` ships an opt-in `debug` feature (off by default, so a release allocator has no
per-allocation cost). With `--features debug`, `DebugAlloc` wraps the size-class allocator and adds:

* **Redzones**: `guard = align_up(8, align)` bytes of a fixed pattern on each side of every object,
  checked on free; a leading/trailing mismatch counts as underflow/overflow.
* **Freed-pattern poison**: freed objects are overwritten with `0xde` (and fresh ones with `0xba`),
  so a use-after-free read returns a recognizable byte.
* **Allocation audit**: a 256-entry ring of `AuditRecord { op, addr, size, seq }`.
* **Double-free**: surfaced through the existing slab free-bitmap (`DoubleFree`) and counted.

`DebugAlloc::realloc` grows/shrinks with a byte copy. The host hammer (`debug_hammer_millions`)
drives a million mixed alloc/free/realloc operations against a real backing buffer, checking byte
patterns and asserting zero redzone/double-free detections on correct use; `failure_injection_*`
pins a tiny arena and asserts `alloc` returns `Exhausted` rather than a bad or duplicate pointer.

## 7. Physical frames: buddy/zone core (A5a, OBSERVED)

`crates/raven-frames` now carries a power-of-two buddy allocator beside the existing bitmap
allocator: `BuddyAllocator<ORDERS, MAX_BLOCKS, P>` manages a caller-supplied power-of-two frame
range with orders `0..=top_order`, all metadata in fixed inline storage (no global allocation).

* **API**: `alloc(order)`, `alloc_at(frame, order)`, `free(frame, order)`, `reserve(frame, count)`,
  `free_blocks(order)`, `free_frames()`, `stats()` and a `Pager` trait (`back`/`unback`) consulted on
  every allocation/free, so the kernel can map/unmap and a host test can inject failure.
* **Errors** (never a panic): `OrderTooLarge`, `OutOfFrames`, `OutOfRange`, `DoubleFree`,
  `Occupied`, `Misaligned`, `Pager`, `ZeroSize`.
* **Invariants** (tested): free + allocated + reserved = total; buddies coalesce on free; the
  exhaustive small-frame model and a 20k-op random model assert conservation and full coalescing
  back to a single top-order block; the OOM path reports `OutOfFrames`.

The per-CPU frame caches and the kernel wiring are A5b, after S75.

## 8. Physical frames: per-CPU magazines (A5b, OBSERVED)

`FrameCache<NCPUS, CACHE_CAP, REMOTE_CAP>` adds the per-CPU fast path over the A5a buddy: fixed
order-0 caches with high/low watermarks, batch refill/drain to a [`FrameSource`] (the buddy), a
bounded remote-free queue per CPU, and per-CPU stats (`hits`, `misses`, `refills`, `drains`,
`remote_frees`, `evictions`). Every call takes an explicit CPU id and the source, so the layer reads
no hardware or globals. The pager failure path is complete: a `Pager` that refuses to back surfaces
`BuddyError::Pager` through `alloc`, and conservation (`free + cached + remote + live + reserved ==
total`) is intact. The 4-CPU million-op model and a single-CPU baseline assert no frame lost or
duplicated, no cache above its capacity, and full coalescing after a drain.

Kernel wiring (A5c) remains, after S75 releases the SMP paths.

## 9. Physical frames: kernel integration (A5c, OBSERVED)

`kernel/src/memory/frame.rs` now routes every page allocation through the A5a buddy and the A5b
per-CPU magazines; the old bitmap path is gone from the kernel.

* **Range and reservations**: the buddy manages `[0, next_pow2(highest usable end))`, capped at
  `MAX_BLOCKS = 1<<17` order-0 descriptors (512 MiB). Every non-`Usable` region from the boot map,
  every gap between regions, and the padding to the power-of-two end is `reserve`d, so kernel,
  loader, ACPI, firmware and MMIO frames are never handed out. The descriptor array lives in
  `.bss` (`BlockStorage`/`Block`), so construction never puts ~1 MiB on the 8 MiB boot stack.
* **Per-CPU path**: `FrameCache<MAX_CPUS, 64, 32>` is indexed by `smp::percpu::this_cpu()`; one
  global spin mutex guards the buddy and the cache together, so a refill/drain and a foreign free
  cannot race. `frames_percpu=off` (or `0`/`false`/`no`) drains the caches and routes allocations
  straight to the buddy for debugging.
* **DMA contiguity**: `alloc_contiguous` takes each frame of the run as an independent order-0
  block (`alloc_at`), so the existing DMA teardown that frees a buffer frame by frame keeps
  working; an occupied frame rolls the probe back and the scan moves to the next aligned run.
* **Reclaim**: `free_range` releases reserved frames through `BuddyAllocator::release`, which
  drops the reservation count and coalesces. Loader-reclaimable frames thus return to the buddy
  once paging copied the boot data.
* **Statistics**: `stats()` reports `total_bytes`, `free_bytes` (buddy free + cached),
  `cached_bytes`, `reserved_bytes`, `allocated_bytes` and `regions`; `cpu_stats(cpu)` exposes the
  per-CPU hit/miss/refill/drain/remote/eviction counters.
* **Soak**: `frame::selftest` (`frames_selftest=1`) runs a 256-frame alloc/free probe with the
  caches on and off, a 32-frame contiguous run freed frame by frame, and an OOM/conservation probe;
  `cargo xtask qtest --case kmem-frames` runs it in QEMU. A 1,000,000-frame host model in
  `buddy_ut.rs` exercises the allocator at the 4 GiB equivalent size.

Known limits: the 512 MiB cap is a descriptor-storage constant, not an allocator limit; the
per-CPU caches share one lock today (correctness first, contention reduction is a follow-up); and
a double free of a frame still sitting in a CPU cache is not detected (the buddy detects double
frees once the frame is back in its free lists).

## 10. What A1..A6 defer to A5c+

The architecture names three allocation policies and a quantum/import hierarchy. A1 ships
`FirstFit`, `BestFit` and `NextFit`; the `InstantFit` power-of-two freelists and quantum caches are
not part of A1's contract and belong with the kmem hot path (where the real per-CPU preemption and
contention counters live). A1 also keeps the import source explicit: the kernel feeds the arena
from the mapped heap range, and the wider kva/ID arenas (R-MEM-3) land with the frame/ID work.
The R-MEM-10 debug bitmask (`REDZONE|POISON|AUDIT|LITE|LEAK`) remains a kernel-side runtime flag;
A6 provides the mechanisms behind a compile-time host/test feature.

The remaining frame work is zones (`dma32`/`normal`) and NUMA node hints from R-MEM-13/14, the
pager-backed frame hooks beyond the existing `BlockIo` path, and moving the per-CPU caches off the
single allocator lock.
