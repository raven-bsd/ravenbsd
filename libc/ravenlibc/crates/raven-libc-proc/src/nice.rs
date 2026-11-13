// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
//
// RavenBSD: transpiled from musl:src/unistd/nice.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `nice` over a priority seam.

use crate::consts::PRIO_PROCESS;
use crate::errors::{EACCES, EPERM, Errno};

/// The traditional nice offset.
pub const NZERO: i32 = 20;
const NZERO2: i32 = 40;
const NZERO_MAX: i32 = NZERO - 1;
const NZERO_MIN: i32 = -NZERO;

/// The priority operations the shell provides.
pub trait PrioOps {
    /// Read a priority.
    fn getpriority(&mut self, which: u32, who: i32) -> Result<i32, Errno>;
    /// Set a priority.
    fn setpriority(&mut self, which: u32, who: i32, prio: i32) -> Result<(), Errno>;
}

/// Change the nice value by `inc` and return the clamped result.
pub fn nice(ops: &mut dyn PrioOps, inc: i32) -> Result<i32, Errno> {
    let mut prio = inc;
    if inc > -NZERO2 && inc < NZERO2 {
        let old = ops.getpriority(PRIO_PROCESS, 0)?;
        prio = prio.saturating_add(old);
    }
    prio = prio.clamp(NZERO_MIN, NZERO_MAX);
    match ops.setpriority(PRIO_PROCESS, 0, prio) {
        Ok(()) => Ok(prio),
        Err(EACCES) => Err(EPERM),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "nice_ut.rs"]
mod tests;
