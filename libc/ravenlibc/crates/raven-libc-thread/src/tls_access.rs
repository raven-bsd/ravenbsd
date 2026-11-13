// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/__tls_get_addr.c, src/env/__reset_tls.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `__tls_get_addr`: `dtv[module] + offset`, with an Acquire load of the published pointer.
//!
//! There is no slow path and no allocation (A-tls-2). A module outside the published DTV
//! returns 0 rather than dereferencing a random word.

use core::sync::atomic::Ordering;

use crate::kernel::ThreadKernel;

/// `__tls_get_addr`.
#[must_use]
pub fn tls_get_addr<K: ThreadKernel>(kernel: &K, module: usize, offset: usize) -> usize {
    let pthread = crate::thread::current_pthread(kernel);
    let pointer = pthread.dtv.load(Ordering::Acquire);
    if pointer.is_null() {
        return 0;
    }
    raven_libc_tls::dtv::lookup(crate::raw::dtv_ref(pointer as usize), module, offset)
}

/// Re-initialise the calling thread's TLS block after `fork`.
pub fn reset_tls<K: ThreadKernel>(kernel: &K) {
    let modules = crate::thread::create::recorded_modules();
    let count = crate::thread::create::module_count();
    crate::raw::reset_tls_now(kernel.thread_pointer(), modules.get(..count).unwrap_or(&[]));
}

/// The DTV word count of the current thread (tests).
#[must_use]
pub fn module_count<K: ThreadKernel>(kernel: &K) -> usize {
    let pthread = crate::thread::current_pthread(kernel);
    let pointer = pthread.dtv.load(Ordering::Acquire);
    if pointer.is_null() {
        return 0;
    }
    crate::raw::dtv_ref(pointer as usize).count()
}

#[cfg(test)]
#[path = "tls_access_ut.rs"]
mod tests;
