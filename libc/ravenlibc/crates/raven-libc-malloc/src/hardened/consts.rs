// SPDX-License-Identifier: ISC
// Copyright (c) 2008, 2010, 2011, 2016, 2023 Otto Moerbeek <otto@drijf.net>
// Copyright (c) 2012 Matthew Dempsky <matthew@openbsd.org>
// Copyright (c) 2008 Damien Miller <djm@openbsd.org>
// Copyright (c) 2000 Poul-Henning Kamp <phk@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from openbsd:lib/libc/stdlib/malloc.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: malloc.c,v 1.302 2026/11/04 12:37:25 otto Exp $)
// The full ISC permission text and the Beerware remark are in hardened/mod.rs.

//! Constants of the hardened backend, transcribed from OpenBSD `malloc.c`.

/// Minimum chunk shift.
pub const MIN_SHIFT: usize = 4;
/// Minimum chunk size.
pub const MIN_SIZE: usize = 1 << MIN_SHIFT;
/// Maximum chunk shift.
pub const MAX_SHIFT: usize = crate::consts::PAGE_SHIFT - 1;
/// Largest chunk served from a chunk page (2048).
pub const MAX_CHUNK: usize = 1 << MAX_SHIFT;
/// Number of chunk buckets, including bucket 0 and the 2048-byte bucket.
pub const BUCKETS: usize = MAX_CHUNK / MIN_SIZE + 1;
/// Delayed-free slots minus one.
pub const DELAYED_MASK: usize = 15;
/// Delayed-free slots.
pub const DELAYED: usize = DELAYED_MASK + 1;
/// Maximum pools.
pub const MAX_POOLS: usize = 32;
/// Default pools.
pub const DEFAULT_POOLS: usize = 8;
/// Default page-cache slots.
pub const DEFAULT_CACHE: usize = 64;
/// Maximum page-cache slots.
pub const MAX_CACHE: usize = 256;
/// Junk byte written after allocation.
pub const JUNK_ALLOC: u8 = 0xdb;
/// Junk byte written on free.
pub const JUNK_FREE: u8 = 0xdf;
/// The free junk pattern as a word.
pub const JUNK_FREE_WORD: u64 = 0xdfdfdfdfdfdfdfdf;
/// Bytes of the trailing canary checked at free.
pub const CHECK_LENGTH: usize = 32;
/// Region slots in a fresh table.
pub const INITIAL_REGIONS: usize = 256;
/// Maximum region slots.
pub const MAX_REGIONS: usize = 4096;
/// Chunk metadata records per 64-page batch.
pub const CHUNK_PAGES: usize = 64;
/// Maximum chunks in one 4 KiB page.
pub const MAX_CHUNKS: usize = crate::consts::PAGE_SIZE / MIN_SIZE;
/// Bytes reserved for one chunk metadata record.
pub const CHUNK_INFO_SIZE: usize = 640;
/// Cache entries kept directly in a pool.
pub const MAX_CACHE_ENTRIES: usize = 64;

/// Bytes represented by bucket `bucket` (bucket 0 is the minimum allocation).
#[must_use]
pub const fn bucket_bytes(bucket: usize) -> usize {
    if bucket == 0 {
        MIN_SIZE
    } else {
        bucket * MIN_SIZE
    }
}

/// The bucket for a request; a zero request uses bucket 0.
#[must_use]
pub const fn find_bucket(request: usize) -> usize {
    if request == 0 {
        return 0;
    }
    let size = if request < MIN_SIZE {
        MIN_SIZE
    } else {
        request
    };
    size.div_ceil(MIN_SIZE)
}
