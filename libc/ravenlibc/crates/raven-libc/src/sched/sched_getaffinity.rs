// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sched_getaffinity(2)`.
use core::ffi::c_int;

/// Report all CPUs: CPU 0 in the caller's `cpu_set_t`.
///
/// # Safety
/// `mask` points to `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sched_getaffinity(pid: c_int, size: usize, mask: *mut u8) -> c_int {
    if mask.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let backend = crate::ffi::procbackend::backend();
    if let Err(error) = crate::ffi::map_proc::pid_exists(&backend, pid) {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error)));
    }
    // SAFETY: the caller's writable `size` bytes.
    let set = unsafe { crate::ffi::cast::bytes_mut(mask.cast(), size) };
    let result = crate::ffi::map_proc::fill_affinity(set);
    crate::ffi::ret::int(result.map(|()| 0).map_err(crate::ffi::map_proc::error))
}
