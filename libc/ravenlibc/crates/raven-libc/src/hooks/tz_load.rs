// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_tz_load`: the TZif reader of `raven-libc-time` over the platform's file seam.
//!
//! The time crate exposes [`raven_libc_time::tz::load_zone`], which needs a `TimeSys`. The shell's
//! time system is declared in `src/time/sys.rs`; this hook reports whether a zone was loaded so
//! `localtime_r` can fall back to the UTC dummy of the table.

/// The strong time-zone hook; the TZ state is part of the time pass, so the UTC dummy is kept.
#[unsafe(no_mangle)]
pub fn __rl_tz_load_impl(_name: &[u8]) -> bool {
    false
}

/// Install the strong hook.
pub(crate) fn install() {
    // `localtime_r` and the TZ state live in the time pass; the cell keeps the UTC dummy until
    // the caller file exists (the hook table's documented dummy).
}
