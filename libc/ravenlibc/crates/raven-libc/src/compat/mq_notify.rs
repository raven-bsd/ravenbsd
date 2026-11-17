// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_notify`. No descriptor can name a queue, so the answer is `EBADF`.

use core::ffi::{c_int, c_void};

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_notify`.
///
/// # Safety
/// `notification` is NULL or points to a readable `struct sigevent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_notify(_mqd: c_int, _notification: *const c_void) -> c_int {
    fail(mqueue::queue_descriptor_error())
}
