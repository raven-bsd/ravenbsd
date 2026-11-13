// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/__tls_get_addr.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The dynamic thread vector.
//!
//! The ABI is musl's flexible array: word 0 is the module count and word `i` (1-based) is
//! `block_start_i + DTP_OFFSET`. `Dtv` is a view of word 0; the words that follow are addressed
//! through [`lookup`], which bounds-checks against word 0 and then performs the audited pointer
//! access in [`crate::raw`].
//!
//! Publication is Release and the lookup load is Acquire (A-tls-1). A generation counter outside
//! the array records each publication; it cannot live inside the array because word 0 is the
//! module count the loader and the TLS-descriptor assembly read.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::layout::LayoutError;

/// Generation of the latest DTV publication.
static GENERATION: AtomicU32 = AtomicU32::new(0);

/// The first word of a DTV (`dtv[0]`).
#[repr(C)]
pub struct Dtv {
    /// Number of module entries that follow this word.
    pub count: AtomicUsize,
}

impl Dtv {
    /// A view of a DTV whose word 0 is `count`.
    #[must_use]
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// The address of word 0 (the pointer the TLS-descriptor assembly indexes).
    #[must_use]
    pub fn words(&self) -> *const AtomicUsize {
        self as *const Self as *const AtomicUsize
    }
}

/// Publication generation; incremented by [`publish`].
#[must_use]
pub fn generation() -> u32 {
    GENERATION.load(Ordering::Acquire)
}

/// Record one publication.
pub(crate) fn bump_generation() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// The address stored in `dtv[module]`, or `None` for a module the DTV does not describe.
///
/// `offset` is the byte offset within the module (`dtv[module] + offset`).
#[must_use]
pub fn lookup(dtv: &Dtv, module: usize, offset: usize) -> usize {
    if module == 0 || module > dtv.count() {
        return 0;
    }
    crate::raw::dtv_word(dtv, module).wrapping_add(offset)
}

/// The same lookup with an explicit failure instead of a zero result.
///
/// # Errors
///
/// [`LayoutError::TooLarge`] when `module` is outside the published DTV.
pub fn try_lookup(dtv: &Dtv, module: usize, offset: usize) -> Result<usize, LayoutError> {
    if module == 0 || module > dtv.count() {
        return Err(LayoutError::TooLarge);
    }
    Ok(crate::raw::dtv_word(dtv, module).wrapping_add(offset))
}

/// Publish a grown DTV with Release ordering (A-tls-1).
///
/// # Safety
///
/// `new` must point to a live word array whose word 0 is the module count and whose words
/// `1..=count` are valid `block + DTP_OFFSET` addresses, and it must stay alive for as long as a
/// thread can load it from `slot`.
pub use crate::raw::publish_dtv as publish;

/// Load the published DTV pointer with Acquire ordering.
pub use crate::raw::load_dtv as load;

#[cfg(test)]
#[path = "dtv_ut.rs"]
mod tests;
