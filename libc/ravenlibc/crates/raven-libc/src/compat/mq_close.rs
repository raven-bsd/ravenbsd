// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_close`. No descriptor can name a queue, so the answer is `EBADF`.

use core::ffi::c_int;

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_close`.
///
/// # Safety
/// Scalar argument only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_close(_mqd: c_int) -> c_int {
    fail(mqueue::queue_descriptor_error())
}
