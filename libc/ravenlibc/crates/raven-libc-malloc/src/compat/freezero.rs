// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `freezero` for a heap without the native size-checked path.

use crate::compat::Heap;
use crate::raw;

/// Zero `size` bytes at `pointer` and free it; a null pointer is a no-op.
pub fn freezero<H: Heap>(heap: &H, pointer: usize, size: usize) {
    if pointer == 0 {
        return;
    }
    // SAFETY: the caller passed a live allocation of at least `size` bytes, the contract of
    // `freezero`; a short buffer is undefined in C and aborts in the hardened backend.
    raw::fill_volatile(pointer, 0, size);
    heap.free(pointer);
}

#[cfg(test)]
#[path = "freezero_ut.rs"]
mod tests;
