// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_self(3)`.
use raven_libc_thread::ThreadKernel;
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::thread::join;

/// The calling thread's TCB pointer, or 0 before the thread pointer is installed.
#[unsafe(no_mangle)]
pub extern "C" fn pthread_self() -> usize {
    if SysKernel.thread_pointer() == 0 {
        return 0;
    }
    join::self_thread(&SysKernel)
}
