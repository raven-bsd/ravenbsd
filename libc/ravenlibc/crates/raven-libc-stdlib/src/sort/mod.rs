// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Sorting and searching arrays: the OpenBSD introsort `qsort`, the allocation-free in-place
//! heap fallback, the BSD `heapsort`/`mergesort` extensions and `bsearch`.
//!
//! The engines index a `&mut [u8]`; the shell computes `nmemb * size` with `checked_mul` once
//! and passes a slice of exactly that length (A-stdlib-1, T-1, T-3). Comparators are closures
//! returning the C sign value and are called in the donor's order for `qsort`/`bsearch`.

pub mod bsearch;
pub mod heap_fallback;
pub mod heapsort;
pub mod mergesort;
pub mod qsort;

pub use bsearch::bsearch;
pub use heapsort::heapsort;
pub use mergesort::mergesort;
pub use qsort::{STABLE_QSORT, qsort};

/// Why a BSD sorter refused.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum SortError {
    /// An element size or length the donor rejects with `EINVAL`.
    Inval,
    /// The temporary buffer could not be allocated.
    NoMem,
}

/// The size of a pointer in the donor's linked-list shading (`PSIZE`).
pub(crate) const PSIZE: usize = core::mem::size_of::<usize>();

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
