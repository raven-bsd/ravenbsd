# kmem: kernel allocator design

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: PROPOSED. The `raven-vmem` arena (A1) is implemented; `raven-kmem` (A2+) is the next layer.

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

## 3. What A1 defers to A2+

The architecture names three allocation policies and a quantum/import hierarchy. A1 ships
`FirstFit`, `BestFit` and `NextFit`; the `InstantFit` power-of-two freelists and quantum caches are
not part of A1's contract and belong with the kmem hot path (A3, where the `O(1)` fast path and its
contention counters are specified). A1 also keeps the import source explicit: the caller passes the
range, and the kernel-integrating ticket will feed it from the kva/ID arenas (R-MEM-3). There is no
global or static state, and no release hook yet; both land when the arena is wired to real mapping.

## 4. kmem next steps (A2..A6, PROPOSED)

* **A2** object caches over vmem with the general size classes and the `GlobalAlloc` shim
  (R-MEM-5, R-MEM-8).
* **A3** per-CPU magazines and the depot layer (R-MEM-6, R-MEM-7).
* **A4** kernel integration replacing `kernel/src/memory/heap.rs` once the kernel lane releases
  that path (R-MEM-3, R-MEM-23, R-MEM-24).
* **A5** per-CPU frame caches, buddy/zones and pager hooks (R-MEM-13..15).
* **A6** debug modes behind `kmem_flags` (R-MEM-10).
