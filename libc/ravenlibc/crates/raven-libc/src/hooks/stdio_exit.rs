// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_stdio_exit`: flushes every stream and closes the open-file list.

/// The strong definition pulled by the stdio read/write cores.
#[unsafe(no_mangle)]
pub fn __rl_stdio_exit_impl() {
    let streams = crate::stdio::streams::streams();
    for file in streams.all() {
        let _ = raven_libc_stdio::flush::fflush(file.state(), Some(file));
    }
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::stdlib::exit::__rl_stdio_exit::install(Some(__rl_stdio_exit_impl));
}
