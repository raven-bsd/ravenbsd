// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inotify_rm_watch(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealFdTable, RealWatch, ret_unit};

/// Remove a watch; `ENOSYS` until PG-38.
#[unsafe(no_mangle)]
pub extern "C" fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int {
    ret_unit(raven_libc_personality_linux::inotify::rm_watch(
        &mut RealFdTable,
        &mut RealWatch,
        fd,
        wd,
    ))
}
