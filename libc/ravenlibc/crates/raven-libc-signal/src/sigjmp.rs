// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigsetjmp_tail.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/siglongjmp.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The `setjmp` family and its mask tail.
//!
//! `setjmp` never saves the mask (A-sig-5); `sigsetjmp(buf, 1)` saves it in `__ss[0]` and
//! restores it in the returned-to context through the asm tail, so the restore happens on the
//! stack that is live after the jump. The asm of the three architectures lives in
//! `raw/setjmp_*` and `raw/sigsetjmp_*`; `longjmp` restores registers only and never runs
//! cancellation cleanups.

use raven_libc_abi::structs::JmpBuf;

use crate::raw;

/// `setjmp` returns 0.
pub const SETJMP: i32 = 0;
/// `longjmp(buf, 0)` returns 1.
pub const LONGJMP: i32 = 1;

/// `setjmp`: save the callee-saved context.
#[inline(always)]
pub fn setjmp(buffer: &mut JmpBuf) -> i32 {
    raw::setjmp_save(buffer)
}

/// `longjmp`: restore a saved context; `value == 0` becomes 1. Never returns.
#[inline(always)]
pub fn longjmp(buffer: &JmpBuf, value: i32) -> ! {
    raw::longjmp_restore(buffer, value)
}

/// `sigsetjmp(buf, savemask)` with the calling thread's TCB mask word.
#[inline(always)]
pub fn sigsetjmp(buffer: &mut JmpBuf, savemask: i32) -> i32 {
    #[cfg(not(test))]
    {
        raw::sigsetjmp_current(buffer, savemask)
    }
    #[cfg(test)]
    {
        // Host unit tests must not touch the real `%fs` TLS of the test runtime. The asm-level
        // test drives `sigsetjmp_with_mask` with an explicit word; this path uses a crate-local
        // word so a unit test can call `sigsetjmp` without corrupting the host thread.
        static TEST_MASK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
        let mut word = TEST_MASK.load(core::sync::atomic::Ordering::Acquire);
        let result = raw::sigsetjmp_save(buffer, savemask, &mut word);
        TEST_MASK.store(word, core::sync::atomic::Ordering::Release);
        result
    }
}

/// `sigsetjmp` against an explicit mask word (the shell and the asm tests use this form).
///
#[inline(always)]
pub fn sigsetjmp_with_mask(buffer: &mut JmpBuf, savemask: i32, mask: &mut u64) -> i32 {
    raw::sigsetjmp_save(buffer, savemask, mask)
}

/// `siglongjmp`: the mask restore happens in the returned-to context.
#[inline(always)]
pub fn siglongjmp(buffer: &JmpBuf, value: i32) -> ! {
    longjmp(buffer, value)
}

/// The tail's logic as safe Rust: `ret == 0` stores the live word into `__ss[0]`, otherwise the
/// saved word is restored; the return value is `ret`.
pub fn sigsetjmp_tail(buffer: &mut JmpBuf, ret: i32, mask: &mut u64) -> i32 {
    if ret == 0 {
        if let Some(slot) = buffer.bytes.get_mut(raw::JMP_SS_OFFSET >> 3) {
            *slot = *mask;
        }
    } else if let Some(saved) = buffer.bytes.get(raw::JMP_SS_OFFSET >> 3).copied() {
        *mask = saved;
    }
    ret
}

#[cfg(test)]
#[path = "sigjmp_ut.rs"]
mod sigjmp_ut;
