// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

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
    ))
}
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
