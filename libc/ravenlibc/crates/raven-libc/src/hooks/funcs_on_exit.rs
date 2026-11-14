// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_funcs_on_exit`: the `atexit` registry lives in `raven-libc-stdlib`; the start
//! path installs this member and `exit` calls the hook, which runs the registered handlers.

/// The strong definition pulled by `atexit`/`__cxa_atexit`/`on_exit`.
#[unsafe(no_mangle)]
pub fn __rl_funcs_on_exit_impl() {
    // The registry instance is the shell's `exit_registry`; the C41 table does the work.
    crate::exit_registry::run_all();
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::stdlib::exit::__rl_funcs_on_exit::install(Some(__rl_funcs_on_exit_impl));
}
