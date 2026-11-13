// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/process/fork.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `fork`, the cooperative lock registry and the child-side repair (H-21, A-thr-9).
//!
//! The registry is a fixed, rank-sorted array of `&'static AtomicI32`. `fork` takes the user
//! prepare handlers, blocks app signals, takes every registered lock in rank order, forks,
//! releases in reverse in the parent, and in the child STORES 0 into every registered word
//! (never unlocks), marks the other TCBs `tid = -1`, sets the thread count to 1, resets the
//! list lock and TLS, and only then runs the child handlers. Without PG-45 `fork` is `ENOSYS`.

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering};

use crate::kernel::ThreadKernel;
use crate::libc_state::LIBC;
use crate::limits::{ENOSYS, Errno, REGISTRY_MAX};
use crate::raw;
use crate::thread::{atfork, list, sigmask};

static REG_PTR: [AtomicUsize; REGISTRY_MAX] = [const { AtomicUsize::new(0) }; REGISTRY_MAX];
static REG_RANK: [AtomicU8; REGISTRY_MAX] = [const { AtomicU8::new(0) }; REGISTRY_MAX];
static REG_LEN: AtomicUsize = AtomicUsize::new(0);
static REG_LOCK: AtomicBool = AtomicBool::new(false);

fn registry_lock() {
    while REG_LOCK
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
}

fn registry_unlock() {
    REG_LOCK.store(false, Ordering::Release);
}

/// Register a lock with its section 4.7 rank (1..9).
pub fn register_lock(lock: &'static AtomicI32, rank: u8) {
    registry_lock();
    let length = REG_LEN.load(Ordering::Acquire);
    if length < REGISTRY_MAX {
        let mut index = length;
        let mut insert_at = length;
        while index > 0 {
            index -= 1;
            let other = REG_RANK
                .get(index)
                .map_or(0, |slot| slot.load(Ordering::Acquire));
            if other <= rank {
                insert_at = index + 1;
                break;
            }
            insert_at = index;
        }
        let mut cursor = length;
        while cursor > insert_at {
            if let (Some(ptr_dst), Some(ptr_src), Some(rank_dst), Some(rank_src)) = (
                REG_PTR.get(cursor),
                REG_PTR.get(cursor - 1),
                REG_RANK.get(cursor),
                REG_RANK.get(cursor - 1),
            ) {
                ptr_dst.store(ptr_src.load(Ordering::Acquire), Ordering::Release);
                rank_dst.store(rank_src.load(Ordering::Acquire), Ordering::Release);
            }
            cursor -= 1;
        }
        if let (Some(ptr), Some(rank_slot)) = (REG_PTR.get(insert_at), REG_RANK.get(insert_at)) {
            ptr.store(lock as *const AtomicI32 as usize, Ordering::Release);
            rank_slot.store(rank, Ordering::Release);
        }
        REG_LEN.store(length + 1, Ordering::Release);
    }
    registry_unlock();
}

/// Clear the registry (startup and each serialised host test).
pub fn reset_registry() {
    REG_LEN.store(0, Ordering::Release);
    REG_LOCK.store(false, Ordering::Release);
}

/// Number of registered locks.
#[must_use]
pub fn registry_len() -> usize {
    REG_LEN.load(Ordering::Acquire)
}

fn registered(index: usize) -> Option<&'static AtomicI32> {
    let address = REG_PTR.get(index)?.load(Ordering::Acquire);
    if address == 0 {
        None
    } else {
        Some(raw::atomic_i32_static(address))
    }
}

fn take_locks<K: ThreadKernel>(kernel: &K) {
    for index in 0..registry_len() {
        if let Some(word) = registered(index) {
            crate::sync::lock::lock(kernel, word);
        }
    }
}

fn store_zero_all() {
    for index in 0..registry_len() {
        if let Some(word) = registered(index) {
            word.store(0, Ordering::Release);
        }
    }
}

fn release_locks<K: ThreadKernel>(kernel: &K) {
    let mut index = registry_len();
    while index > 0 {
        index -= 1;
        if let Some(word) = registered(index) {
            crate::sync::lock::unlock(kernel, word);
        }
    }
}

/// `fork` (the full sequence).
///
/// # Errors
///
/// `ENOSYS` without PG-45 and the kernel's fork error.
pub fn fork<K: ThreadKernel>(kernel: &K) -> Result<i32, Errno> {
    if !kernel.caps().proc_fork {
        return Err(ENOSYS);
    }
    atfork::lock_handlers(kernel);
    atfork::run_prepare();
    let old_mask = sigmask::block_app_sigs(kernel);
    let multi = LIBC.threads() > 1;
    if multi {
        take_locks(kernel);
        list::lock(kernel);
    }
    let result = kernel.fork_process();
    match result {
        Ok(0) => {
            let self_thread = crate::thread::current_pthread(kernel);
            let self_address = self_thread as *const raven_libc_tls::Pthread as usize;
            let mut current = raw::next_thread(self_thread);
            while current as *const raven_libc_tls::Pthread as usize != self_address {
                current.tid.store(-1, Ordering::Release);
                current = raw::next_thread(current);
            }
            raw::set_links(self_thread, self_address, self_address);
            store_zero_all();
            list::reset_for_fork_child();
            LIBC.set_threads(1);
            LIBC.set_need_locks(0);
            self_thread.tid.store(LIBC.next_tid(), Ordering::Release);
            crate::tls_access::reset_tls(kernel);
            atfork::run_after(true);
            atfork::unlock_handlers(kernel);
            sigmask::restore_sigs(kernel, old_mask);
            Ok(0)
        }
        Ok(pid) => {
            if multi {
                list::unlock(kernel);
                release_locks(kernel);
            }
            sigmask::restore_sigs(kernel, old_mask);
            atfork::run_after(false);
            atfork::unlock_handlers(kernel);
            Ok(pid)
        }
        Err(error) => {
            if multi {
                list::unlock(kernel);
                release_locks(kernel);
            }
            sigmask::restore_sigs(kernel, old_mask);
            atfork::unlock_handlers(kernel);
            Err(error)
        }
    }
}

/// `_Fork`: fork without the user handlers.
///
/// # Errors
///
/// `ENOSYS` without PG-45.
pub fn fork_raw<K: ThreadKernel>(kernel: &K) -> Result<i32, Errno> {
    if !kernel.caps().proc_fork {
        return Err(ENOSYS);
    }
    let old_mask = sigmask::block_app_sigs(kernel);
    let multi = LIBC.threads() > 1;
    if multi {
        take_locks(kernel);
        list::lock(kernel);
    }
    let result = kernel.fork_process();
    match result {
        Ok(0) => {
            store_zero_all();
            list::reset_for_fork_child();
            LIBC.set_threads(1);
            LIBC.set_need_locks(0);
            Ok(0)
        }
        Ok(pid) => {
            if multi {
                list::unlock(kernel);
                release_locks(kernel);
            }
            sigmask::restore_sigs(kernel, old_mask);
            Ok(pid)
        }
        Err(error) => {
            if multi {
                list::unlock(kernel);
                release_locks(kernel);
            }
            sigmask::restore_sigs(kernel, old_mask);
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "fork_ut.rs"]
mod tests;

/// Take the registered locks and the thread-list lock before a fork (the `fork` prologue).
pub fn fork_prepare<K: ThreadKernel>(kernel: &K) {
    if LIBC.threads() > 1 {
        take_locks(kernel);
        list::lock(kernel);
    }
}

/// Release the locks in reverse order after a parent-side fork.
pub fn fork_parent<K: ThreadKernel>(kernel: &K) {
    if LIBC.threads() > 1 {
        list::unlock(kernel);
        release_locks(kernel);
    }
}

/// Store 0 into every registered word, forget the other TCBs and reset the process state.
pub fn fork_child<K: ThreadKernel>(kernel: &K) {
    let self_thread = crate::thread::current_pthread(kernel);
    let self_address = self_thread as *const raven_libc_tls::Pthread as usize;
    let mut current = raw::next_thread(self_thread);
    while current as *const raven_libc_tls::Pthread as usize != self_address {
        current.tid.store(-1, Ordering::Release);
        current = raw::next_thread(current);
    }
    raw::set_links(self_thread, self_address, self_address);
    store_zero_all();
    list::reset_for_fork_child();
    LIBC.set_threads(1);
    LIBC.set_need_locks(0);
    self_thread.tid.store(LIBC.next_tid(), Ordering::Release);
    crate::tls_access::reset_tls(kernel);
}
