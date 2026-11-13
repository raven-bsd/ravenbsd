<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-malloc

The allocator core of ravenlibc (`architecture/raven-libc/donor-map/08-malloc-memory.md`). It is
`no_std`, contains no `extern crate alloc`, and keeps every raw memory access in one audited module,
`src/raw.rs`. Two backends implement the same `Heap` trait:

* `hardened` — the OpenBSD `malloc.c` design (ISC; PHK's Beerware remark kept verbatim): pools,
  delayed free, junk fill, optional canaries and guard pages.
* `tiny` — the picolibc small allocator design (BSD-3, ARM Ltd): one address-ordered free list,
  zeroing `malloc`, `map_exact`/`map_pages` growth.

`compat` carries the C-shaped layer (`reallocarray`, `recallocarray`, `reallocf`, `freezero`,
overflow checks, `mallinfo`, `malloc_usable_size`), `api` implements the `Api` semantics of doc 08
section 4.4, and `debug` is a libumem-inspired quarantine/redzone front end behind the `debug`
feature.

Build and test: `cargo test -p raven-libc-malloc`; debug: `cargo test -p raven-libc-malloc
--features debug`; model: `cargo test -p raven-libc-malloc --release --test model`.

Not implemented (documented deviations, not hidden): per-CPU magazines, vmem arenas, slab caches
(`raven-umem`), OpenBSD `MALLOC_STATS`/return-address leak reports, debug guard pages that require
a second mapping, and the picolibc bucket lists (`SMALL_BUCKET = 0`). The hardened backend keeps the
OpenBSD page cache limit and the 16-slot delayed free.

Provenance: the hardened backend is derived from `openbsd:lib/libc/stdlib/malloc.c`. The tiny
backend follows the picolibc sources named in the ticket. Any file copied from a donor carries the
donor notice and an `[[import]]` row in the root `PROVENANCE.toml`.
