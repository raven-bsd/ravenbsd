// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread lifecycle: create, exit, join, detach, attributes, stacks, keys, cancellation.

use core::sync::atomic::Ordering;

use raven_libc_tls::Pthread;

use crate::kernel::ThreadKernel;
use crate::raw;

pub mod atfork;
pub mod attr;
pub mod cancel;
pub mod create;
pub mod exit;
pub mod join;
pub mod kill;
pub mod list;
pub mod name;
pub mod once;
pub mod sched;
pub mod sigmask;
pub mod stack;
pub mod tsd;

pub use create::{create, init_main_thread};
pub use exit::exit;
pub use join::{detach, join, tryjoin};

/// The calling thread's TCB.
///
/// The kernel guarantees the thread pointer is live while the thread runs.
#[must_use]
pub fn current_pthread<K: ThreadKernel>(kernel: &K) -> &'static Pthread {
    raw::tcb(raven_libc_tls::tcb::tp_to_pthread(kernel.thread_pointer()))
}

/// The calling thread's TCB with mutable access.
///
/// # Panics
///
/// Never in shipped code; the pointer is the thread's own TCB.
#[must_use]
pub fn current_pthread_mut<K: ThreadKernel>(kernel: &K) -> &'static mut Pthread {
    raw::tcb_mut(raven_libc_tls::tcb::tp_to_pthread(kernel.thread_pointer()))
}

/// The calling thread's 30-bit tid (0 when the TCB exists but has not been assigned yet).
#[must_use]
pub fn current_tid<K: ThreadKernel>(kernel: &K) -> i32 {
    let tp = kernel.thread_pointer();
    if tp == 0 {
        return kernel.tid_self().unwrap_or(0);
    }
    current_pthread(kernel).tid.load(Ordering::Acquire) & 0x3fff_ffff
}

/// The entry trampoline stored in `StartArgs` and called by the assembly stub.
pub fn thread_start<K: ThreadKernel>(kernel: &K, args: usize) -> ! {
    create::thread_start(kernel, args)
}
