// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/ldso/tlsdesc.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! TLS descriptor glue: the two assembly entries and the dynamic resolver.
//!
//! The exported `__tlsdesc_static`/`__tlsdesc_dynamic` names are aliases the shell makes of the
//! hidden `__rl_*` symbols (RD-new 5). The resolver path is used only where the arch has no
//! direct descriptor call.

use crate::kernel::ThreadKernel;

/// The static descriptor entry: `descriptor[1]`.
#[must_use]
pub fn static_entry(descriptor: &mut [usize; 2]) -> usize {
    crate::raw::tlsdesc_static(descriptor)
}

/// The dynamic descriptor entry.
#[must_use]
pub fn dynamic_entry(descriptor: &mut [usize; 2]) -> usize {
    crate::raw::tlsdesc_dynamic(descriptor)
}

/// The `{module, offset}` resolver used by the loader when it cannot call the asm entry.
#[must_use]
pub fn resolver<K: ThreadKernel>(kernel: &K, descriptor: &[usize; 2]) -> usize {
    crate::tls_access::tls_get_addr(kernel, descriptor[0], descriptor[1])
}

#[cfg(test)]
#[path = "tlsdesc_ut.rs"]
mod tests;
