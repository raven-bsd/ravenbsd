// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_funcs_on_exit`: the `atexit` registry lives in `raven-libc-stdlib`; the start
//! path installs this member and `exit` calls the hook, which runs the registered handlers.

/// The strong definition pulled by `atexit`/`__cxa_atexit`/`on_exit`.
#[unsafe(no_mangle)]
pub fn __rl_funcs_on_exit_impl() {
    // The registry is owned by C41 and its process instance is C61's start object. The member
    // exists so the hook cell can be installed; the body stays empty until that instance is
    // handed over (C61 section 4.3 item 9).
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::stdlib::exit::__rl_funcs_on_exit::install(Some(__rl_funcs_on_exit_impl));
}
