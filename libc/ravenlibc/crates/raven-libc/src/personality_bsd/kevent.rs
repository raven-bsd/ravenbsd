// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `kevent(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Timespec;
use raven_libc_personality_bsd::kqueue::Kevent;
use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::{
    KQUEUES, RealEvqBsd, RealFdTableBsd, RealSrc, ret_count, timespec,
};

/// Submit changes and wait for events.
///
/// # Safety
/// The pointer/length pairs name readable/writable arrays (or NULL with zero length); `timeout`
/// is NULL or a readable `struct timespec`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kevent(
    kq: c_int,
    changes: *const Kevent,
    nchanges: c_int,
    events: *mut Kevent,
    nevents: c_int,
    timeout: *const Timespec,
) -> c_int {
    if nchanges < 0 || nevents < 0 {
        return ret_count(Err(Errno::EINVAL));
    }
    let nchanges = usize::try_from(nchanges).unwrap_or(0);
    let nevents = usize::try_from(nevents).unwrap_or(0);
    if (changes.is_null() && nchanges > 0) || (events.is_null() && nevents > 0) {
        return ret_count(Err(Errno::EFAULT));
    }
    // SAFETY: the caller granted `nchanges` readable and `nevents` writable records.
    let changes = unsafe { core::slice::from_raw_parts(changes, nchanges) };
    // SAFETY: as above.
    let events = unsafe { core::slice::from_raw_parts_mut(events, nevents) };
    let timeout = unsafe { timespec(timeout) };
    ret_count(KQUEUES.with(|table| {
        raven_libc_personality_bsd::kqueue::kevent(
            &mut RealEvqBsd,
            &RealFdTableBsd,
            &mut RealSrc,
            table,
            kq,
            changes,
            events,
            timeout,
        )
    }))
}
