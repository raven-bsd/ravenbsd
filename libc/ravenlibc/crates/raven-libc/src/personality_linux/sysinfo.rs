// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sysinfo(2)`.
use core::ffi::c_int;

use raven_libc_personality_linux::seams::Errno;
use raven_libc_personality_linux::sysinfo::LinuxSysinfo;

use crate::personality_common::{RealSysInfo, ret_unit};

/// Fill a `struct sysinfo`; `ENOSYS` until PG-21.
///
/// # Safety
/// `out` points to one writable `struct sysinfo`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysinfo(out: *mut LinuxSysinfo) -> c_int {
    if out.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    let page_size = crate::start::page_size();
    let result = raven_libc_personality_linux::sysinfo::fill(&mut RealSysInfo, page_size);
    if let Ok(value) = result {
        // SAFETY: the caller granted one writable struct sysinfo.
        unsafe { core::ptr::write(out, value) };
    }
    ret_unit(result.map(|_| ()))
}
