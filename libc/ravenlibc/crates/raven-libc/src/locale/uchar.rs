// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared helpers for the `<uchar.h>` codec exports.

use raven_libc_ctype::MbState;

/// Use the caller's state or a local initial state.
///
/// # Safety
/// `state` is NULL or writable.
pub(crate) unsafe fn state_or_local<'a>(
    state: *mut MbState,
    local: &'a mut MbState,
) -> &'a mut MbState {
    // SAFETY: the caller's `mbstate_t` contract.
    unsafe { state.as_mut() }.unwrap_or(local)
}

/// The C `size_t` form of a reader return value.
pub(crate) const fn size(value: isize) -> usize {
    value as usize
}
