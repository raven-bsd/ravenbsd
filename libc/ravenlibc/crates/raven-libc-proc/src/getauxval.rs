// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
//
// RavenBSD: transpiled from musl:src/misc/getauxval.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `getauxval` over the parsed auxiliary vector.

use crate::auxv::AuxTable;
use crate::consts::AT_SECURE;
use crate::errors::{ENOENT, Errno};

/// Return one auxiliary-vector value, or `ENOENT` when absent.
///
/// `AT_SECURE` is answered from the libc's own flag, never from the table.
pub fn getauxval(table: &AuxTable, key: usize, secure: bool) -> Result<usize, Errno> {
    if key == AT_SECURE {
        return Ok(usize::from(secure));
    }
    table.get(key).ok_or(ENOENT)
}

#[cfg(test)]
#[path = "getauxval_ut.rs"]
mod tests;
