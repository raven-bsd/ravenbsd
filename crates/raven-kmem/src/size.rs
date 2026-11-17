// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! General-purpose size classes and their selection.

#![forbid(unsafe_code)]

/// The general-purpose size classes, in ascending order.
///
/// These are the power-of-two-ish classes the [`GlobalAlloc`](core::alloc::GlobalAlloc) shim
/// routes `alloc` to (R-MEM-8). A request at or below [`LARGE_THRESHOLD`] that fits a class is
/// served from that class's cache; larger requests go straight to the source arena.
pub const SIZE_CLASSES: [usize; 19] = [
    8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 8192,
];

/// Largest class size. Requests larger than this bypass the classes.
pub const LARGE_THRESHOLD: usize = 8192;

/// The slab alignment a class cache uses.
///
/// Classes 8 and 16 align to their own size; every larger class aligns to 16 bytes, the natural
/// alignment of the widest primitive. A request that needs stricter alignment than its class
/// offers is served straight from the arena.
pub const fn class_align(class_size: usize) -> usize {
    if class_size <= 16 { class_size } else { 16 }
}

/// Index into [`SIZE_CLASSES`] of the smallest class that can hold `size`, or `None` when `size`
/// is above [`LARGE_THRESHOLD`].
pub fn class_for(size: usize) -> Option<usize> {
    SIZE_CLASSES.iter().position(|&class| class >= size)
}

/// Which class serves an allocation of `size` bytes with `align` alignment.
///
/// `None` means the request bypasses the classes and goes straight to the source arena: either it
/// is above [`LARGE_THRESHOLD`], or its alignment is stricter than any class offers. The function
/// is pure, so an `alloc`/`free` pair for the same `(size, align)` always agrees.
pub fn classify(size: usize, align: usize) -> Option<usize> {
    if align > 16 {
        return None;
    }
    let index = class_for(size)?;
    if align <= class_align(SIZE_CLASSES[index]) {
        Some(index)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "size_ut.rs"]
mod tests;
