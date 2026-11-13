// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Chunk headers and address arithmetic for the tiny backend.
//!
//! The layout is the picolibc one: a `usize` size word immediately before the user pointer, with
//! bit 0 marking a free chunk.

use crate::consts::align_up;
use crate::raw;
use crate::tiny::config::MALLOC_CHUNK_ALIGN;

/// Size of the header word.
pub const HEAD_SIZE: usize = core::mem::size_of::<usize>();
/// Minimum chunk including the header.
pub const CHUNK_MIN: usize = 16;
/// Maximum chunk size accepted by the donor.
pub const CHUNK_MAX: usize = (usize::MAX - HEAD_SIZE) & !(MALLOC_CHUNK_ALIGN - 1);
/// Maximum user allocation.
pub const ALLOC_MAX: usize = CHUNK_MAX - HEAD_SIZE;

/// Address of the size word for a chunk.
#[must_use]
pub fn size_ref(chunk: usize) -> usize {
    chunk.wrapping_sub(HEAD_SIZE)
}

/// The size word, including the free bit.
#[must_use]
pub fn size_word(chunk: usize) -> usize {
    // SAFETY: a chunk always has a readable size word; this is the allocator's basic invariant.
    raw::read_usize(size_ref(chunk))
}

/// The chunk size without the free bit.
#[must_use]
pub fn size(chunk: usize) -> usize {
    size_word(chunk) & !1
}

/// Set a busy size.
pub fn set_size(chunk: usize, value: usize) {
    // SAFETY: every chunk has a writable size word.
    raw::write_usize(size_ref(chunk), value);
}

/// Mark a chunk free.
pub fn mark_free(chunk: usize) {
    // SAFETY: every chunk has a writable size word.
    raw::write_usize(size_ref(chunk), size(chunk) | 1);
}

/// Mark a chunk busy.
pub fn mark_busy(chunk: usize) {
    set_size(chunk, size(chunk));
}

/// Whether the free bit is set.
#[must_use]
pub fn is_free(chunk: usize) -> bool {
    size_word(chunk) & 1 != 0
}

/// End of the chunk including its header.
#[must_use]
pub fn chunk_end(chunk: usize) -> usize {
    size_ref(chunk).wrapping_add(size(chunk))
}

/// Address of the next chunk in memory.
#[must_use]
pub fn chunk_after(chunk: usize) -> usize {
    chunk.wrapping_add(size(chunk))
}

/// Convert a user pointer to its chunk address.
#[must_use]
pub fn ptr_to_chunk(pointer: usize) -> usize {
    pointer
}

/// Convert a chunk address to its user pointer.
#[must_use]
pub fn chunk_to_ptr(chunk: usize) -> usize {
    chunk
}

/// Convert a region base to the first chunk, aligning the user address to 16 bytes.
#[must_use]
pub fn blob_to_chunk(blob: usize) -> usize {
    let with_header = blob.wrapping_add(HEAD_SIZE);
    align_up(with_header, MALLOC_CHUNK_ALIGN).unwrap_or(with_header)
}

/// Convert a chunk to its region base.
#[must_use]
pub fn chunk_to_blob(chunk: usize) -> usize {
    chunk.wrapping_sub(HEAD_SIZE)
}

/// Round a user request to a chunk size.
#[must_use]
pub fn chunk_size(request: usize) -> Option<usize> {
    let with_header = request.checked_add(HEAD_SIZE)?;
    let aligned = align_up(with_header, MALLOC_CHUNK_ALIGN)?;
    Some(if aligned < CHUNK_MIN {
        CHUNK_MIN
    } else {
        aligned
    })
}

/// Usable bytes in a chunk.
#[must_use]
pub fn usable(chunk: usize) -> usize {
    size(chunk).saturating_sub(HEAD_SIZE)
}

#[cfg(test)]
#[path = "chunk_ut.rs"]
mod tests;
