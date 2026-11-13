// SPDX-License-Identifier: BSD-3-Clause
/*
 * Copyright (c) 1983, 1993
 *	The Regents of the University of California.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from openbsd:lib/libc/gen/nice.c @
// 4ce71f2817f675745b0596aaf04de4a122b1cc52; the getpriority/setpriority walk
// is kept and the clamp to -NZERO..NZERO-1 happens before the set instead of
// after it.

//! `nice` and the priority clamp (T8).

use raven_abi::errno::Errno;

/// `NZERO` for Raven (doc 11 section 3.2).
pub const NZERO: i32 = 20;

/// Clamp a priority BEFORE it reaches `setpriority`; the donor clamps after the set, which
/// procd rejects with `EINVAL` (row 9 trap).
pub fn clamp_priority(value: i32) -> i32 {
    value.clamp(-NZERO, NZERO - 1)
}

/// `nice(incr)` over a caller-supplied priority backend.
///
/// Until `procd` exists (O-6): a non-negative increment is a no-op success at the current
/// priority and a negative one is `EPERM`.
pub fn nice(
    increment: i32,
    current: Option<i32>,
    set: impl FnOnce(i32) -> Result<(), Errno>,
) -> Result<i32, Errno> {
    match current {
        None => {
            if increment < 0 {
                Err(Errno::EPERM)
            } else {
                Ok(0)
            }
        }
        Some(priority) => {
            let target = clamp_priority(priority.saturating_add(increment));
            set(target)?;
            Ok(target)
        }
    }
}

#[cfg(test)]
#[path = "nice_ut.rs"]
mod tests;
