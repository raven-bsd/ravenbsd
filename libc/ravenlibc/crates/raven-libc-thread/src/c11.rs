// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/thrd_create.c, thrd_exit.c, thrd_join.c, thrd_sleep.c, thrd_yield.c, mtx_destroy.c, mtx_init.c, mtx_lock.c, mtx_timedlock.c, mtx_trylock.c, mtx_unlock.c, cnd_broadcast.c, cnd_destroy.c, cnd_init.c, cnd_signal.c, cnd_timedwait.c, cnd_wait.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! C11 `<threads.h>`: thin wrappers over the pthread core and the C11 error codes.
//!
//! `thrd_start_t` is spelled `fn(usize) -> usize` here; the shell casts the C function pointer
//! and narrows the result to `int`.

use crate::kernel::ThreadKernel;
use crate::limits::{
    EAGAIN, EBUSY, ETIMEDOUT, PTHREAD_CREATE_JOINABLE, PTHREAD_MUTEX_NORMAL, Timespec,
};
use crate::mutex::Mutex;
use crate::thread::once::Once;

/// `thrd_t`.
pub type Thrd = usize;
/// `thrd_start_t`.
pub type ThrdStart = fn(usize) -> usize;
/// `thrd_success`.
pub const THRD_SUCCESS: i32 = 0;
/// `thrd_busy`.
pub const THRD_BUSY: i32 = 1;
/// `thrd_error`.
pub const THRD_ERROR: i32 = 2;
/// `thrd_nomem`.
pub const THRD_NOMEM: i32 = 3;
/// `thrd_timedout`.
pub const THRD_TIMEDOUT: i32 = 4;

fn map(error: crate::limits::Errno) -> i32 {
    if error == EAGAIN {
        THRD_NOMEM
    } else if error == EBUSY {
        THRD_BUSY
    } else if error == ETIMEDOUT {
        THRD_TIMEDOUT
    } else {
        THRD_ERROR
    }
}

/// `thrd_create`.
pub fn thrd_create<K: ThreadKernel>(kernel: &K, start: ThrdStart, arg: usize) -> Result<Thrd, i32> {
    crate::thread::create::create(kernel, None, start, arg).map_err(map)
}

/// `thrd_join`.
///
/// # Errors
///
/// The C11 error code.
pub fn thrd_join<K: ThreadKernel>(kernel: &K, thread: Thrd) -> Result<usize, i32> {
    crate::thread::join::join(kernel, thread).map_err(map)
}

/// `thrd_detach`.
///
/// # Errors
///
/// The C11 error code.
pub fn thrd_detach<K: ThreadKernel>(kernel: &K, thread: Thrd) -> Result<(), i32> {
    crate::thread::join::detach(kernel, thread).map_err(map)
}

/// `thrd_exit`.
pub fn thrd_exit<K: ThreadKernel>(kernel: &K, result: i32) -> ! {
    crate::thread::exit::exit(kernel, result as isize as usize)
}

/// `thrd_sleep`: the milliseconds are rounded up to the next yield; the fake has no sleep.
pub fn thrd_sleep<K: ThreadKernel>(
    kernel: &K,
    _duration: Timespec,
    _remaining: Option<&mut Timespec>,
) -> i32 {
    kernel.yield_now();
    0
}

/// `thrd_yield`.
pub fn thrd_yield<K: ThreadKernel>(kernel: &K) {
    kernel.yield_now();
}

/// `thrd_current`.
#[must_use]
pub fn thrd_current<K: ThreadKernel>(kernel: &K) -> Thrd {
    crate::thread::join::self_thread(kernel)
}

/// `thrd_equal`.
#[must_use]
pub fn thrd_equal(a: Thrd, b: Thrd) -> bool {
    a == b
}

/// `mtx_init`.
pub fn mtx_init(mutex: &mut Mutex, kind: i32) -> i32 {
    if kind < 0 {
        return THRD_ERROR;
    }
    crate::mutex::init(mutex, None);
    let _ = PTHREAD_MUTEX_NORMAL;
    THRD_SUCCESS
}

/// `mtx_destroy`.
pub fn mtx_destroy(_mutex: &Mutex) {}

/// `mtx_lock`.
pub fn mtx_lock<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> i32 {
    crate::mutex::lock(kernel, mutex).map_or(THRD_ERROR, |()| THRD_SUCCESS)
}

/// `mtx_trylock`.
pub fn mtx_trylock<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> i32 {
    match crate::mutex::trylock(kernel, mutex) {
        Ok(()) => THRD_SUCCESS,
        Err(error) => map(error),
    }
}

/// `mtx_timedlock`.
pub fn mtx_timedlock<K: ThreadKernel>(kernel: &K, mutex: &Mutex, at: Timespec) -> i32 {
    match crate::mutex::timedlock(kernel, mutex, Some(at)) {
        Ok(()) => THRD_SUCCESS,
        Err(error) => map(error),
    }
}

/// `mtx_unlock`.
pub fn mtx_unlock<K: ThreadKernel>(kernel: &K, mutex: &Mutex) -> i32 {
    crate::mutex::unlock(kernel, mutex).map_or(THRD_ERROR, |()| THRD_SUCCESS)
}

/// `cnd_init`.
pub fn cnd_init(cond: &mut crate::cond::Cond) -> i32 {
    crate::cond::init(cond, None);
    THRD_SUCCESS
}

/// `cnd_destroy`.
pub fn cnd_destroy(_cond: &crate::cond::Cond) {}

/// `cnd_wait`.
pub fn cnd_wait<K: ThreadKernel>(kernel: &K, cond: &crate::cond::Cond, mutex: &Mutex) -> i32 {
    crate::cond::wait(kernel, cond, mutex).map_or(THRD_ERROR, |()| THRD_SUCCESS)
}

/// `cnd_timedwait`.
pub fn cnd_timedwait<K: ThreadKernel>(
    kernel: &K,
    cond: &crate::cond::Cond,
    mutex: &Mutex,
    at: Timespec,
) -> i32 {
    match crate::cond::timedwait(kernel, cond, mutex, cond.clock(), Some(at)) {
        Ok(()) => THRD_SUCCESS,
        Err(error) => map(error),
    }
}

/// `cnd_signal`.
pub fn cnd_signal<K: ThreadKernel>(kernel: &K, cond: &crate::cond::Cond) -> i32 {
    crate::cond::signal(kernel, cond).map_or(THRD_ERROR, |()| THRD_SUCCESS)
}

/// `cnd_broadcast`.
pub fn cnd_broadcast<K: ThreadKernel>(kernel: &K, cond: &crate::cond::Cond) -> i32 {
    crate::cond::broadcast(kernel, cond).map_or(THRD_ERROR, |()| THRD_SUCCESS)
}

/// `call_once`.
pub fn call_once<K: ThreadKernel>(kernel: &K, flag: &Once, function: fn()) {
    crate::thread::once::call_once(kernel, flag, function);
}

/// `tss_create`.
pub fn tss_create<K: ThreadKernel>(kernel: &K, destructor: Option<fn(usize)>) -> (i32, u32) {
    match crate::thread::tsd::key_create(kernel, destructor) {
        Ok(key) => (THRD_SUCCESS, key),
        Err(error) => (map(error), 0),
    }
}

/// `tss_delete`.
pub fn tss_delete<K: ThreadKernel>(kernel: &K, key: u32) {
    crate::thread::tsd::key_delete(kernel, key);
}

/// `tss_get`.
#[must_use]
pub fn tss_get<K: ThreadKernel>(kernel: &K, key: u32) -> usize {
    crate::thread::tsd::getspecific(kernel, key)
}

/// `tss_set`.
pub fn tss_set<K: ThreadKernel>(kernel: &K, key: u32, value: usize) -> i32 {
    crate::thread::tsd::setspecific(kernel, key, value).map_or(THRD_ERROR, |()| THRD_SUCCESS)
}

/// `thrd_create`'s sentinel detach state helper: C11 threads are joinable by default.
pub const THRD_JOINABLE: i32 = PTHREAD_CREATE_JOINABLE;

#[cfg(test)]
#[path = "c11_ut.rs"]
mod tests;
