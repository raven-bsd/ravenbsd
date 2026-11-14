// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inotify_init1(2)`.
use core::ffi::c_int;

use crate::personality_common::{RealFdTable, RealWatch, ret_int};

/// Create an inotify instance with flags; `ENOSYS` until PG-38 after flag validation.
#[unsafe(no_mangle)]
pub extern "C" fn inotify_init1(flags: c_int) -> c_int {
    ret_int(raven_libc_personality_linux::inotify::init1(
        &mut RealFdTable,
        &mut RealWatch,
        flags,
    ))
}
