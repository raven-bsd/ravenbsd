// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pselect`.

use super::*;

/// `pselect` over file descriptor sets with an atomic signal mask.
///
/// # Safety
/// The sets are NULL or writable; `timeout`/`sigmask` are NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pselect(
    nfds: c_int,
    readfds: *mut FdSet,
    writefds: *mut FdSet,
    exceptfds: *mut FdSet,
    timeout: *const Timespec,
    sigmask: *const SigsetT,
) -> c_int {
    // SAFETY: each NULL set is an empty slice; otherwise it is 16 writable words.
    let read = unsafe { fd_set(readfds) };
    // SAFETY: same contract as `read`.
    let write = unsafe { fd_set(writefds) };
    // SAFETY: same contract as `read`.
    let except = unsafe { fd_set(exceptfds) };
    // SAFETY: NULL is `None`.
    let timeout = unsafe { timeout.as_ref() };
    // SAFETY: NULL is `None`; otherwise the caller supplied a valid SigsetT.
    let mask = unsafe { sigmask.as_ref() }.map(|value| &value.bytes[..]);
    int(raven_libc_platform::poll::pselect(
        global(),
        nfds,
        read,
        write,
        except,
        timeout,
        mask,
        &mut raven_libc_platform::poll::KernelMasks,
    ))
}
