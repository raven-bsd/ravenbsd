// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 1998 M. Warner Losh <imp@FreeBSD.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: derived from freebsd:lib/libc/stdlib/reallocf.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// The BSD-2-Clause permission text is the donor's header; the port keeps the control flow.

//! FreeBSD `reallocf`: free the original block when `realloc` fails.

use crate::compat::Heap;

/// `reallocf(p, n)`: on failure the original block is freed.
pub fn reallocf<H: Heap>(heap: &H, pointer: usize, size: usize) -> usize {
    let result = heap.realloc(pointer, size);
    if result == 0 && pointer != 0 && size != 0 {
        heap.free(pointer);
    }
    result
}

#[cfg(test)]
#[path = "reallocf_ut.rs"]
mod tests;
