// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tiny-backend build constants (the picolibc `picolibc.h` values fixed by doc 08 3.4).

/// Allocated-block alignment on every target (T-16).
pub const MALLOC_CHUNK_ALIGN: usize = 16;
/// Small-bucket lists are disabled in the first import.
pub const SMALL_BUCKET: usize = 0;
/// The smallest growth request, in bytes.
pub const TINY_GROW_MIN: usize = 64 * 1024;
/// The nominal page granule.
pub const PAGE_ALIGN: usize = crate::consts::PAGE_SIZE;
