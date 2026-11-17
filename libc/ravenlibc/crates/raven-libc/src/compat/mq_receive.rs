// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_receive`. A null buffer is `EFAULT`; otherwise the descriptor is `EBADF`. Nothing is
//! delivered.

use core::ffi::{c_char, c_int};

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_receive`.
///
/// # Safety
/// `message` points to at least `length` writable bytes; `priority` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_receive(
    _mqd: c_int,
    message: *mut c_char,
    _length: usize,
    _priority: *mut u32,
) -> isize {
    if message.is_null() {
        let _ = fail(raven_libc_ipc::EFAULT);
        return -1;
    }
    let _ = fail(mqueue::queue_descriptor_error());
    -1
}
