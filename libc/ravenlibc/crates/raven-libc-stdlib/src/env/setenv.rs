// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1987, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/stdlib/setenv.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: setenv.c,v 1.45 2021/08/12 20:53:18 andvar Exp $)
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

//! `setenv`; the no-free deviation of D-2 lives in [`EnvTable::setenv`].

use raven_libc_abi::errno::Errno;

use super::table::EnvTable;

/// `setenv(name, value, overwrite)`.
///
/// # Errors
///
/// `EINVAL` for an empty name or one containing `=`, `ENOMEM` on allocation failure.
pub fn setenv(env: &mut EnvTable, name: &[u8], value: &[u8], overwrite: bool) -> Result<(), Errno> {
    env.setenv(name, value, overwrite)
}

#[cfg(test)]
#[path = "setenv_ut.rs"]
mod tests;
