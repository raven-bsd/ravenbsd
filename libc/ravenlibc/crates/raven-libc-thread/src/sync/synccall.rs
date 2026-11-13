// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/synccall.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `__synccall`: run a callback on every thread.
//!
//! The donor sends `SIGSYNCCALL` to every kernel tid and waits on three semaphores. Without PG-7
//! (`THREAD_KICK`) there is no signal path: the callback runs only on the caller and the function
//! documents that the `setuid` family affects only the calling thread. With PG-7 the crate kicks
//! every thread handle; because C58 owns the signal trampoline, the callback itself still runs
//! on the caller in this crate and the trampoline is the shell's responsibility.

use crate::kernel::ThreadKernel;
use crate::libc_state::LIBC;
use crate::raw;

/// Signal bit used by the donor for the synchronised call (`SIGSYNCCALL`).
pub const SIGSYNCCALL_BIT: u64 = 1 << 33;

/// Kick every other thread and run `func` on the caller.
///
/// Returns the number of successful kicks (0 without PG-7).
pub fn synccall<K: ThreadKernel>(kernel: &K, func: impl FnOnce()) -> usize {
    let mut kicked = 0;
    if kernel.caps().kick {
        let main = LIBC.main_tcb();
        if main != 0 {
            let first = raw::tcb(main);
            let mut current = first;
            loop {
                let handle = current.handle;
                if handle != 0 && kernel.kick(handle, SIGSYNCCALL_BIT).is_ok() {
                    kicked += 1;
                }
                current = raw::next_thread(current);
                if core::ptr::eq(current, first) {
                    break;
                }
            }
        }
    }
    func();
    kicked
}

#[cfg(test)]
#[path = "synccall_ut.rs"]
mod tests;
