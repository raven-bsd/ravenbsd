// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `select`.

use super::*;

/// `select` over file descriptor sets.
///
/// # Safety
/// The three sets are NULL or writable; `timeout` is NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn select(
    nfds: c_int,
    readfds: *mut FdSet,
    writefds: *mut FdSet,
    exceptfds: *mut FdSet,
    timeout: *const Timeval,
) -> c_int {
    // SAFETY: each NULL set is an empty slice; otherwise it is 16 writable words.
    let read = unsafe { fd_set(readfds) };
    // SAFETY: same contract as `read`.
    let write = unsafe { fd_set(writefds) };
    // SAFETY: same contract as `read`.
    let except = unsafe { fd_set(exceptfds) };
    // SAFETY: NULL is `None`.
    let timeout = unsafe { timeout.as_ref() };
    int(raven_libc_platform::poll::select(
        global(),
        nfds,
        read,
        write,
        except,
        timeout,
    ))
}
