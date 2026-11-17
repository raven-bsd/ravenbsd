// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_send`. A null message is `EFAULT`; otherwise the descriptor is `EBADF`. Nothing is
//! queued.

use core::ffi::{c_char, c_int};

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_send`.
///
/// # Safety
/// `message` points to at least `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_send(
    _mqd: c_int,
    message: *const c_char,
    _length: usize,
    _priority: u32,
) -> c_int {
    if message.is_null() {
        return fail(raven_libc_ipc::EFAULT);
    }
    fail(mqueue::queue_descriptor_error())
}
