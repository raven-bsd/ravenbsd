// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mq_timedreceive`. A null buffer is `EFAULT`; otherwise the descriptor is `EBADF`. Nothing is
//! delivered.

use core::ffi::{c_char, c_int, c_void};

use raven_libc_ipc::mqueue;

use super::fail;

/// `mq_timedreceive`.
///
/// # Safety
/// `message` points to at least `length` writable bytes; `priority` and `timeout` are NULL or
/// readable/writable as their direction requires.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mq_timedreceive(
    _mqd: c_int,
    message: *mut c_char,
    _length: usize,
    _priority: *mut u32,
    _timeout: *const c_void,
) -> isize {
    if message.is_null() {
        let _ = fail(raven_libc_ipc::EFAULT);
        return -1;
    }
    let _ = fail(mqueue::queue_descriptor_error());
    -1
}
