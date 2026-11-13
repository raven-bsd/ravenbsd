// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/wait.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// Copyright (c) 1988, 1993
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! `wait` over a `wait4` seam.

use crate::errors::Errno;

/// Wait for any child.
pub const WAIT_ANY: i32 = -1;

/// The `wait4` operation the shell implements.
pub trait Wait4 {
    /// Wait for `pid` with `options`, optionally filling a `rusage` record.
    fn wait4(
        &mut self,
        pid: i32,
        options: u32,
        rusage: Option<&mut [u8; 144]>,
    ) -> Result<i32, Errno>;
}

/// Wait for any child.
pub fn wait<W: Wait4>(waiter: &mut W) -> Result<i32, Errno> {
    waiter.wait4(WAIT_ANY, 0, None)
}

#[cfg(test)]
#[path = "wait_ut.rs"]
mod tests;
