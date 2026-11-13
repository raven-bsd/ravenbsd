// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_malloc_impl`: the C49 allocator front end.
//!
//! The dummy is the bump allocator of the hook table (the `tiny` profile); this member is pulled
//! by `free`/`realloc`/`posix_memalign` and by `-u __rl_malloc_impl_impl`, and is installed by
//! C61's start path. The exported `malloc` family calls the same C49 object directly
//! ([`crate::malloc::heap::api`]), so a program that installs this hook and a program that does
//! not share one heap.

/// The strong allocator hook.
#[unsafe(no_mangle)]
pub fn __rl_malloc_impl_impl(size: usize) -> usize {
    crate::malloc::heap::api().malloc(size)
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::malloc::malloc_fn::__rl_malloc_impl::install(Some(__rl_malloc_impl_impl));
}
