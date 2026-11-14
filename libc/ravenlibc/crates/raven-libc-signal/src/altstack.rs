// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigaltstack.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `sigaltstack`: libc-only bookkeeping in the TCB (TBD-9, 4.6.3).
//!
//! musl validates against `sysconf(_SC_MINSIGSTKSZ)` and calls `SYS_sigaltstack`; Raven's kernel
//! saves no context on the user stack (PG-7), so the whole call is three TCB words. `SS_ONSTACK`
//! is reported live by comparing `sp` with the registered range. A size below `MINSIGSTKSZ`
//! (6144 from C37; PG-50 would add `AT_MINSIGSTKSZ`) is `ENOMEM`, a change while on the stack is
//! `EPERM`, and the `SS_ONSTACK` input flag is `EINVAL` as in the donor.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::StackT;

use crate::SigKernel;
use crate::errors::{EINVAL_ERR, ENOMEM_ERR, EPERM_ERR};
use crate::raw;

/// The C37 minimum alternate-stack size.
pub const MINSIGSTKSZ: usize = signal::MINSIGSTKSZ as usize;

/// `SS_ONSTACK`.
pub const SS_ONSTACK: i32 = signal::SS_ONSTACK;
/// `SS_DISABLE`.
pub const SS_DISABLE: i32 = signal::SS_DISABLE;

/// Whether `sp` currently lies inside the thread's enabled alternate stack.
#[must_use]
pub fn on_altstack<K: SigKernel>(kernel: &K, sp: usize) -> bool {
    let (base, size, flags) = raw::altstack_get(kernel.thread_pointer());
    if flags & SS_DISABLE != 0 || size == 0 {
        return false;
    }
    sp >= base && sp < base.wrapping_add(size)
}

/// Query or replace the calling thread's alternate signal stack.
pub fn sigaltstack<K: SigKernel>(
    kernel: &K,
    new: Option<&StackT>,
    old: Option<&mut StackT>,
) -> Result<(), Errno> {
    let tp = kernel.thread_pointer();
    if let Some(stack) = new {
        if stack.ss_flags & SS_ONSTACK != 0 {
            return Err(EINVAL_ERR);
        }
        if stack.ss_flags & SS_DISABLE == 0 && (stack.ss_size as usize) < MINSIGSTKSZ {
            return Err(ENOMEM_ERR);
        }
        let (base, size, flags) = raw::altstack_get(tp);
        if flags & SS_DISABLE == 0 && size != 0 {
            let current_sp = crate::context::current_stack_pointer();
            if current_sp >= base && current_sp < base.wrapping_add(size) {
                return Err(EPERM_ERR);
            }
        }
        if stack.ss_flags & SS_DISABLE != 0 {
            raw::altstack_set(tp, 0, 0, SS_DISABLE);
        } else {
            raw::altstack_set(
                tp,
                stack.ss_sp as usize,
                stack.ss_size as usize,
                stack.ss_flags,
            );
        }
    }
    if let Some(out) = old {
        let (base, size, flags) = raw::altstack_get(tp);
        if flags & SS_DISABLE != 0 || size == 0 {
            *out = StackT {
                ss_sp: core::ptr::null_mut(),
                ss_flags: SS_DISABLE,
                ss_size: 0,
            };
        } else {
            let current_sp = crate::context::current_stack_pointer();
            let active = current_sp >= base && current_sp < base.wrapping_add(size);
            *out = StackT {
                ss_sp: base as *mut core::ffi::c_void,
                ss_flags: flags | if active { SS_ONSTACK } else { 0 },
                ss_size: size as u64,
            };
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "altstack_ut.rs"]
mod altstack_ut;
