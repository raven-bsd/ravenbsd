// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_create.c (__pthread_exit, :54-173) @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `pthread_exit`.
//!
//! Order matters and is the donor's: cleanup handlers, TSD destructors, `killlock`, the list
//! lock, `tid = 0` before the unlink, the user-space robust list, the detach-state decision, the
//! unlink, and then the last user stores. `exit_word = 1` (Release) and the joiner wake are the
//! last stores before the kernel call; after it nothing touches the stack (T-thr-3, A-thr-7).

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;
use crate::libc_state::LIBC;
use crate::limits::{EAGAIN, Errno};
use crate::raw;
use crate::sched_point;
use crate::thread::{list, stack, tsd};

/// `pthread_exit`.
pub fn exit<K: ThreadKernel>(kernel: &K, result: usize) -> ! {
    sched_point!(kernel, "exit_enter");
    let self_pthread = crate::thread::current_pthread_mut(kernel);
    let tcb = self_pthread as *const raven_libc_tls::Pthread as usize;
    self_pthread.result = result;
    self_pthread.sigmask.store(u64::MAX, Ordering::Release);
    // Disable cancellation and asynchronous cancellation.
    self_pthread
        .cancel
        .fetch_and(!(crate::limits::CANCEL_ENABLE), Ordering::AcqRel);
    crate::thread::cancel::run_cleanup(kernel);
    tsd::run_dtors(kernel);
    crate::sync::lock::lock(kernel, &self_pthread.killlock);
    list::lock(kernel);
    if core::ptr::eq(raw::next_thread(self_pthread), self_pthread) {
        list::unlock(kernel);
        crate::sync::lock::unlock(kernel, &self_pthread.killlock);
        raw::run_exit_hook(0);
    }
    let state = self_pthread.swap_detach_state(raven_libc_tls::tcb::DT_EXITING, Ordering::AcqRel);
    self_pthread.tid.store(0, Ordering::Release);
    crate::sync::lock::unlock(kernel, &self_pthread.killlock);
    if !kernel.caps().robust {
        process_robust_list(kernel, self_pthread);
    }
    let previous = raw::prev_thread(self_pthread);
    let next = raw::next_thread(self_pthread);
    raw::set_links(
        previous,
        raw::prev_thread(previous) as *const raven_libc_tls::Pthread as usize,
        next as *const raven_libc_tls::Pthread as usize,
    );
    raw::set_links(
        next,
        previous as *const raven_libc_tls::Pthread as usize,
        raw::next_thread(next) as *const raven_libc_tls::Pthread as usize,
    );
    LIBC.remove_thread();
    list::unlock(kernel);
    sched_point!(kernel, "exit_after_unlink");
    if state == raven_libc_tls::tcb::DT_DETACHED && self_pthread.map_base != 0 {
        self_pthread.sigmask.store(u64::MAX, Ordering::Release);
        if kernel.caps().exit_ex {
            kernel.exit_ex(
                0,
                tcb + core::mem::offset_of!(raven_libc_tls::Pthread, tid),
                self_pthread.map_base,
                self_pthread.map_size,
            );
        }
        if stack::push_zombie(self_pthread.map_base, self_pthread.map_size, tcb).is_err() {
            // The zombie cap is the documented leak bound; the stack stays mapped and the
            // process continues.
            let _ = EAGAIN;
        }
    }
    self_pthread.exit_word.store(1, Ordering::Release);
    kernel.futex_wake(raw::as_u32(&self_pthread.exit_word), u32::MAX);
    self_pthread.sigmask.store(u64::MAX, Ordering::Release);
    sched_point!(kernel, "exit_before_kernel");
    kernel.exit(0)
}

/// Walk the user-space robust list, marking each held mutex `OWNER_DIED`.
fn process_robust_list<K: ThreadKernel>(_kernel: &K, self_pthread: &raven_libc_tls::Pthread) {
    let head = raw::robust_head_addr(self_pthread);
    let mut node = raw::robust_head(self_pthread);
    while node != 0 && node != head {
        let next = raw::read_usize(node);
        let lock_address = node.wrapping_sub(32).wrapping_add(4);
        let waiters_address = node.wrapping_sub(32).wrapping_add(8);
        let waiters = raw::read_u32(waiters_address);
        let continuation = raw::read_u32(lock_address) as i32;
        raw::write_u32(lock_address, crate::limits::OWNER_DIED);
        raw::set_robust_pending(self_pthread, node);
        raw::set_robust_head(self_pthread, next);
        raw::set_robust_pending(self_pthread, 0);
        if continuation < 0 || waiters != 0 {
            _kernel.futex_wake(raw::atomic_u32_at(lock_address), 1);
        }
        node = next;
    }
}

/// `pthread_exit` used by the C11 `thrd_exit` path.
pub fn thrd_exit<K: ThreadKernel>(kernel: &K, result: i32) -> ! {
    exit(kernel, result as isize as usize)
}

/// A convenience alias for `pthread_exit`.
pub fn exit_with_code<K: ThreadKernel>(kernel: &K, code: i32) -> ! {
    exit(kernel, code as isize as usize)
}

/// The result value of a thread that exited normally (`Errno` is not used by pthreads).
pub type ExitResult = Result<(), Errno>;

#[cfg(test)]
#[path = "exit_ut.rs"]
mod tests;
