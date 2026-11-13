// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_once.c, call_once.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `pthread_once`: four states (0 first, 1 running, 2 done, 3 running with waiters).
//!
//! A cancelled `init` resets the control word to 0 and wakes every waiter. The fast path sees
//! state 2 with an Acquire load.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::kernel::ThreadKernel;
use crate::raw;
use crate::sched_point;
use crate::sync::wait::{wait, wake};

/// `pthread_once_t` (4 bytes).
#[repr(C)]
pub struct Once {
    /// The control word; `PTHREAD_ONCE_INIT` is 0.
    pub word: AtomicU32,
}

const _: () = assert!(core::mem::size_of::<Once>() == 4);

impl Once {
    /// A statically initialised control word.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            word: AtomicU32::new(0),
        }
    }

    fn atomic(&self) -> &AtomicU32 {
        &self.word
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

fn undo(control: usize) {
    // SAFETY: `control` is the address of a live `Once` while `init` runs.
    let word = raw::atomic_u32_at(control);
    let _ = word.swap(0, Ordering::AcqRel);
}

/// `pthread_once`.
pub fn once<K: ThreadKernel>(kernel: &K, control: &Once, init: fn()) {
    if control.word.load(Ordering::Acquire) == 2 {
        return;
    }
    once_full(kernel, control, init);
}

fn once_full<K: ThreadKernel>(kernel: &K, control: &Once, init: fn()) {
    let word = control.atomic();
    loop {
        match word.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                let mut cleanup = crate::thread::cancel::Cleanup {
                    next: 0,
                    function: undo,
                    arg: word as *const AtomicU32 as usize,
                };
                crate::thread::cancel::push_cleanup(kernel, &mut cleanup);
                init();
                crate::thread::cancel::pop_cleanup(kernel, &mut cleanup, false);
                sched_point!(kernel, "once_between_1_and_2");
                if word.swap(2, Ordering::AcqRel) == 3 {
                    let _ = wake(kernel, raw::as_i32(word), u32::MAX);
                }
                return;
            }
            Err(1) => {
                let _ = word.compare_exchange(1, 3, Ordering::AcqRel, Ordering::Acquire);
            }
            Err(3) => {}
            Err(2) => return,
            Err(_) => return,
        }
        let value = i32::try_from(word.load(Ordering::Acquire)).unwrap_or(3);
        if value != 2 {
            wait(kernel, raw::as_i32(word), None, 3, true);
        }
    }
}

/// `call_once`: the C11 name.
pub fn call_once<K: ThreadKernel>(kernel: &K, flag: &Once, function: fn()) {
    once(kernel, flag, function);
}

#[cfg(test)]
#[path = "once_ut.rs"]
mod tests;
