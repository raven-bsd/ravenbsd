// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_key_create.c, pthread_getspecific.c, pthread_setspecific.c, tss_create.c, tss_delete.c, tss_set.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
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

//! Thread-specific data: a fixed key table and a per-thread word array inside the mapping.
//!
//! A key owns one destructor slot; `key_delete` clears the slot in every thread while holding the
//! key writer lock and the thread-list lock. Destructors run for at most
//! `PTHREAD_DESTRUCTOR_ITERATIONS` rounds, so a destructor that sets another key is handled.
//! `setspecific` on a deleted key is `EINVAL`.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::kernel::ThreadKernel;
use crate::limits::{EAGAIN, EINVAL, Errno, PTHREAD_DESTRUCTOR_ITERATIONS, PTHREAD_KEYS_MAX};
use crate::raw;
use crate::thread::list;

/// The destructor sentinel for "slot taken, no destructor".
const NODTOR: usize = usize::MAX;

static KEYS: [AtomicUsize; PTHREAD_KEYS_MAX] = [const { AtomicUsize::new(0) }; PTHREAD_KEYS_MAX];
static NEXT_KEY: AtomicUsize = AtomicUsize::new(0);

static MAIN_TSD: [AtomicUsize; PTHREAD_KEYS_MAX] =
    [const { AtomicUsize::new(0) }; PTHREAD_KEYS_MAX];

/// The address of the main thread's static TSD array.
#[must_use]
pub fn main_tsd_address() -> usize {
    MAIN_TSD.as_ptr() as usize
}

/// The main thread's static TSD array as a slice.
#[must_use]
pub fn main_tsd() -> &'static [AtomicUsize] {
    &MAIN_TSD
}

/// `pthread_key_create`.
///
/// # Errors
///
/// `EAGAIN` when all `PTHREAD_KEYS_MAX` slots are taken.
pub fn key_create<K: ThreadKernel>(
    kernel: &K,
    destructor: Option<fn(usize)>,
) -> Result<u32, Errno> {
    let _ = crate::rwlock::wrlock(kernel, raw::key_lock());
    let start = NEXT_KEY.load(Ordering::Acquire);
    let mut index = start;
    let result = loop {
        let slot = KEYS.get(index).ok_or(EAGAIN);
        match slot {
            Ok(slot) if slot.load(Ordering::Acquire) == 0 => {
                let dtor = destructor.map_or(NODTOR, |function| function as usize);
                slot.store(dtor, Ordering::Release);
                NEXT_KEY.store(index.wrapping_add(1) % PTHREAD_KEYS_MAX, Ordering::Release);
                break Ok(index as u32);
            }
            Ok(_) => {
                index = index.wrapping_add(1) % PTHREAD_KEYS_MAX;
                if index == start {
                    break Err(EAGAIN);
                }
            }
            Err(error) => break Err(error),
        }
    };
    let _ = crate::rwlock::unlock(kernel, raw::key_lock());
    result
}

/// `pthread_key_delete`: clear the slot in every thread.
pub fn key_delete<K: ThreadKernel>(kernel: &K, key: u32) {
    let index = key as usize;
    if index >= PTHREAD_KEYS_MAX {
        return;
    }
    let _ = crate::rwlock::wrlock(kernel, raw::key_lock());
    list::lock(kernel);
    let main = crate::libc_state::LIBC.main_tcb();
    if main != 0 {
        let first = raw::tcb(main);
        let mut current = first;
        loop {
            raw::set_tsd_value(current, index, 0);
            current = raw::next_thread(current);
            if core::ptr::eq(current, first) {
                break;
            }
        }
    }
    list::unlock(kernel);
    if let Some(slot) = KEYS.get(index) {
        slot.store(0, Ordering::Release);
    }
    let _ = crate::rwlock::unlock(kernel, raw::key_lock());
}

/// `pthread_getspecific`.
#[must_use]
pub fn getspecific<K: ThreadKernel>(kernel: &K, key: u32) -> usize {
    let pthread = crate::thread::current_pthread(kernel);
    raw::tsd_value(pthread, key as usize)
}

/// `pthread_setspecific`.
///
/// # Errors
///
/// `EINVAL` for a key that was deleted or never created.
pub fn setspecific<K: ThreadKernel>(kernel: &K, key: u32, value: usize) -> Result<(), Errno> {
    let index = key as usize;
    if index >= PTHREAD_KEYS_MAX {
        return Err(EINVAL);
    }
    let Some(slot) = KEYS.get(index) else {
        return Err(EINVAL);
    };
    if slot.load(Ordering::Acquire) == 0 {
        return Err(EINVAL);
    }
    let pthread = crate::thread::current_pthread(kernel);
    raw::set_tsd_value(pthread, index, value);
    raw::set_tsd_used(pthread, true);
    Ok(())
}

/// Run the TSD destructors of an exiting thread (up to four rounds).
pub fn run_dtors<K: ThreadKernel>(kernel: &K) {
    for _round in 0..PTHREAD_DESTRUCTOR_ITERATIONS {
        if !crate::thread::current_pthread(kernel).tsd_used {
            return;
        }
        let _ = crate::rwlock::rdlock(kernel, raw::key_lock());
        let pthread = crate::thread::current_pthread(kernel);
        raw::set_tsd_used(pthread, false);
        let mut ran = false;
        for index in 0..PTHREAD_KEYS_MAX {
            let value = raw::tsd_value(pthread, index);
            if value == 0 {
                continue;
            }
            raw::set_tsd_value(pthread, index, 0);
            let destructor = KEYS
                .get(index)
                .map_or(0, |slot| slot.load(Ordering::Acquire));
            if destructor != 0 && destructor != NODTOR {
                let _ = crate::rwlock::unlock(kernel, raw::key_lock());
                raw::call_destructor(destructor, value);
                let _ = crate::rwlock::rdlock(kernel, raw::key_lock());
                ran = true;
            }
        }
        let _ = crate::rwlock::unlock(kernel, raw::key_lock());
        if !ran {
            return;
        }
    }
}

/// `tss_create`: the C11 error mapping.
#[must_use]
pub fn tss_create<K: ThreadKernel>(kernel: &K, destructor: Option<fn(usize)>) -> i32 {
    match key_create(kernel, destructor) {
        Ok(_) => 0,
        Err(_) => 2,
    }
}

/// `tss_delete`.
pub fn tss_delete<K: ThreadKernel>(kernel: &K, key: u32) {
    key_delete(kernel, key);
}

/// `tss_set`: the C11 error mapping.
#[must_use]
pub fn tss_set<K: ThreadKernel>(kernel: &K, key: u32, value: usize) -> i32 {
    match setspecific(kernel, key, value) {
        Ok(()) => 0,
        Err(_) => 2,
    }
}

#[cfg(test)]
#[path = "tsd_ut.rs"]
mod tests;
