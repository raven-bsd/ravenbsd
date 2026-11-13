// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_malloc_debug`: the `MALLOC_OPTIONS` diagnostics member.
//!
//! The dummy is a no-op. The strong member is pulled by `MALLOC_OPTIONS` handling in the
//! allocator; C49's debug wrapper is the caller when the `debug` feature is selected.

/// The strong debug hook: prints one allocator event through the C49 debug wrapper.
#[unsafe(no_mangle)]
pub fn __rl_malloc_debug_impl() {
    // C49's `debug` module owns the event formatting; the shell exports no debug printer yet
    // (C49 section 4 and the C60 report's RD-new line). Keeping the member empty is the
    // documented `MALLOC_OPTIONS`-off behaviour.
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::malloc::malloc_fn::__rl_malloc_debug::install(Some(__rl_malloc_debug_impl));
}
