// SPDX-License-Identifier: BSD-2-Clause
/*
 * Copyright (C) 2008 The Android Open Source Project
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
// RavenBSD: transpiled from bionic:libc/bionic/getcwd.cpp @
// 731631f300090436d7f5df80d50b6275c8c60a93; the four argument cases are kept,
// `__getcwd` is replaced by the libc cwd string and the page-size allocation
// rule by an exact `strlen(cwd) + 1` allocation; too small a buffer is `ERANGE`.

//! `getcwd` argument handling.

use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

/// `getcwd(buf, size)`.
///
/// * `buf != NULL && size == 0` is `EINVAL`;
/// * `buf == NULL` allocates (size 0 means "as needed");
/// * a supplied buffer smaller than `strlen(cwd) + 1` is `ERANGE`;
/// * success returns the NUL-terminated absolute path.
pub fn getcwd(cwd: &[u8], buf: Option<&mut [u8]>, size: usize) -> Result<Option<Vec<u8>>, Errno> {
    let needed = cwd.len().saturating_add(1);
    match buf {
        None => {
            let mut out = vec![0u8; needed.max(size)];
            out.get_mut(..cwd.len())
                .ok_or(Errno::ERANGE)?
                .copy_from_slice(cwd);
            if let Some(slot) = out.get_mut(cwd.len()) {
                *slot = 0;
            }
            if size == 0 {
                out.truncate(needed);
            }
            Ok(Some(out))
        }
        Some(out) => {
            if size == 0 {
                return Err(Errno::EINVAL);
            }
            if out.len() < needed {
                return Err(Errno::ERANGE);
            }
            out.get_mut(..cwd.len())
                .ok_or(Errno::ERANGE)?
                .copy_from_slice(cwd);
            if let Some(slot) = out.get_mut(cwd.len()) {
                *slot = 0;
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
#[path = "getcwd_ut.rs"]
mod tests;
