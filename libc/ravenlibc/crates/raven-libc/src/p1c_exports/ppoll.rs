// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ppoll`.

use super::*;

/// Poll with an atomic signal mask.
///
/// # Safety
/// `fds` has `nfds` writable records; `timeout`/`sigmask` are NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ppoll(
    fds: *mut Pollfd,
    nfds: u64,
    timeout: *const Timespec,
    sigmask: *const SigsetT,
) -> c_int {
    let count = usize::try_from(nfds).unwrap_or(usize::MAX);
    let records = if fds.is_null() || count == 0 {
        &mut []
    } else {
        // SAFETY: the caller supplied `count` writable pollfd records.
        unsafe { core::slice::from_raw_parts_mut(fds, count) }
    };
    // SAFETY: NULL is `None`; otherwise the caller supplied a valid Timespec/SigsetT.
    let timeout = unsafe { timeout.as_ref() };
    // SAFETY: same contract as `timeout`.
    let mask = unsafe { sigmask.as_ref() }.map(|value| &value.bytes[..]);
    ret::platform_int(
        raven_libc_platform::poll::ppoll(global(), records, timeout, mask)
            .map(|ready| c_int::try_from(ready).unwrap_or(c_int::MAX)),
    )
}
