// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/gen/waitid.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
//
// Copyright (c) 2016 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Christos Zoulas.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

//! `waitid` over a `wait6` seam.

use crate::errors::Errno;
use crate::wait_status::{IdType, waitid_info_on_no_child};

/// The `wait6` operation the shell implements.
pub trait Wait6 {
    /// Wait for `idtype`/`id` and fill the opaque `siginfo_t` record.
    fn wait6(
        &mut self,
        idtype: IdType,
        id: i64,
        options: u32,
        info: &mut [u8; 128],
    ) -> Result<i32, Errno>;
}

/// Wait for a child and zero `info` when nothing was ready.
pub fn waitid<W: Wait6>(
    waiter: &mut W,
    idtype: IdType,
    id: i64,
    info: &mut [u8; 128],
    options: u32,
) -> Result<(), Errno> {
    let pid = waiter.wait6(idtype, id, options, info)?;
    if pid == 0 {
        waitid_info_on_no_child(info);
    }
    Ok(())
}

#[cfg(test)]
#[path = "waitid_ut.rs"]
mod tests;
