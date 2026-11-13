// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<stdlib.h>` `<malloc.h>` allocator exports (`02-api-surface` X08 X09 X69 L11).
//!
//! Every body is one call on the C49 `Api` (see [`heap`]); `malloc` goes through the
//! `__rl_malloc_impl` weak hook of the normative table, whose dummy and strong member both use
//! the same heap object, so the two never diverge.

pub(crate) mod aligned_alloc;
pub(crate) mod calloc;
pub(crate) mod free;
pub(crate) mod freezero;
pub(crate) mod global;
pub(crate) mod heap;
#[path = "malloc.rs"]
pub(crate) mod malloc_fn;
pub(crate) mod malloc_usable_size;
pub(crate) mod memalign;
pub(crate) mod posix_memalign;
pub(crate) mod realloc;
pub(crate) mod reallocarray;

/// The `malloc`-compatible function `strdup` takes as its allocator parameter.
///
/// # Safety
/// The returned pointer has `size` writable bytes or is null.
pub(crate) unsafe fn abi_alloc(size: usize) -> *mut u8 {
    heap::api().malloc(size) as *mut u8
}
