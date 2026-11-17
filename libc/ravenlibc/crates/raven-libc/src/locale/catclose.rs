// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `catclose` from `<nl_types.h>`.

use alloc::boxed::Box;
use core::ffi::{c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_locale::nls::Catalog;

use super::catalog::CATD_FAILED;

/// `catclose`.
///
/// # Safety
/// `catd` is an open catalog or the failure sentinel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn catclose(catd: *mut c_void) -> c_int {
    if catd.is_null() || catd as usize == CATD_FAILED {
        crate::ffi::errno::set(Errno(9));
        return -1;
    }
    // SAFETY: `catd` is a Box from `catopen`.
    drop(unsafe { Box::from_raw(catd.cast::<Catalog>()) });
    0
}
