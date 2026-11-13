// SPDX-License-Identifier: BSD-2-Clause
/*
 * Copyright (C) 2019 The Android Open Source Project
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *  * Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *  * Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 * COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 * BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
 * OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
 * AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
 * OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from bionic:libc/bionic/sleep.cpp @
// 731631f300090436d7f5df80d50b6275c8c60a93; the nanosleep loop and the
// remaining-seconds result are kept, the LP32 INT_MAX branch is dropped because
// ravenlibc is LP64 only, and the remaining time is zero until PG-17 adds the
// kernel output (the full request is returned on EINTR).

//! `sleep`.

use raven_abi::errno::Errno;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

/// `sleep(seconds)`: sleep at least `seconds`, returning unslept whole seconds.
pub fn sleep<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    seconds: u32,
) -> Result<u32, Errno> {
    if seconds == 0 {
        return Ok(0);
    }
    let request = u64::from(seconds).saturating_mul(1_000_000_000);
    match platform.backend().nanosleep(request) {
        Ok(()) => Ok(0),
        // PG-17: the kernel does not report the remaining time yet.
        Err(Errno::EINTR) => Ok(seconds),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "sleep_ut.rs"]
mod tests;
