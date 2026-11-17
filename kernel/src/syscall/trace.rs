// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `TRACE_SET`: set the calling thread's current trace id (`R-OBS-6`).

use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;

use crate::sched;
use crate::syscall::SyscallFrame;

/// `trace_set(id: u64) -> 0`.
///
/// Sets the calling thread's current trace id; zero clears it. Unprivileged and allocation-free:
/// the router stamps this id onto the thread's scheme requests and lets a server inherit it for
/// the duration of a call.
pub fn trace_set(frame: &SyscallFrame) -> Result<usize, Errno> {
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    thread.trace_id.store(frame.rdi, Ordering::Release);
    Ok(0)
}
