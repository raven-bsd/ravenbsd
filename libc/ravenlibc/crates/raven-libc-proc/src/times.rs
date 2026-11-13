// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/times.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// Copyright (c) 1990, 1993
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

//! The `times` conversion.

use crate::errors::{EINVAL, EOVERFLOW, Errno};

/// A seconds/microseconds pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeval {
    /// Seconds.
    pub sec: i64,
    /// Microseconds.
    pub usec: i64,
}

/// The `struct tms` result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tms {
    /// User time of the process.
    pub tms_utime: i64,
    /// System time of the process.
    pub tms_stime: i64,
    /// User time of children.
    pub tms_cutime: i64,
    /// System time of children.
    pub tms_cstime: i64,
}

/// Convert one timeval to clock ticks.
pub fn convtck(time: Timeval, clk_tck: u32) -> Result<i64, Errno> {
    if clk_tck == 0 {
        return Err(EINVAL);
    }
    let divisor = 1_000_000u32.checked_div(clk_tck).ok_or(EINVAL)?;
    if divisor == 0 {
        return Err(EINVAL);
    }
    let seconds = time.sec.checked_mul(i64::from(clk_tck)).ok_or(EOVERFLOW)?;
    seconds
        .checked_add(time.usec.checked_div(i64::from(divisor)).ok_or(EINVAL)?)
        .ok_or(EOVERFLOW)
}

/// Fill `tms` and return the monotonic tick count.
#[allow(clippy::too_many_arguments)] // mirrors the four rusage fields plus the clock
pub fn times(
    tms: Option<&mut Tms>,
    own_utime: Timeval,
    own_stime: Timeval,
    child_utime: Timeval,
    child_stime: Timeval,
    monotonic: Timeval,
    clk_tck: u32,
) -> Result<i64, Errno> {
    let utime = convtck(own_utime, clk_tck)?;
    let stime = convtck(own_stime, clk_tck)?;
    let cutime = convtck(child_utime, clk_tck)?;
    let cstime = convtck(child_stime, clk_tck)?;
    let elapsed = convtck(monotonic, clk_tck)?;
    if let Some(tms) = tms {
        tms.tms_utime = utime;
        tms.tms_stime = stime;
        tms.tms_cutime = cutime;
        tms.tms_cstime = cstime;
    }
    Ok(elapsed)
}

#[cfg(test)]
#[path = "times_ut.rs"]
mod tests;
