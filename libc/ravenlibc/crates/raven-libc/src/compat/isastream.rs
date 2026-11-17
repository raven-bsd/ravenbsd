// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `isastream`. RavenBSD has no STREAMS, so a valid descriptor answers "not a stream".

use core::ffi::c_int;

use raven_libc_ipc::stropts;

use super::fail;

/// `isastream`.
///
/// # Safety
/// Scalar argument only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn isastream(fd: c_int) -> c_int {
    match stropts::isastream(fd) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(error) => fail(error),
    }
}
