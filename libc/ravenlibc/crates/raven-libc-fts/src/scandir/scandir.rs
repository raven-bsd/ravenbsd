// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/dirent/scandir.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header upstream; covered by musl/COPYRIGHT, H-1)
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

//! `scandir`: collect a directory into a sorted `Vec<DirEntry>`.
//!
//! The donor caps at `INT_MAX` entries with `EOVERFLOW`, guards the allocation size and sorts
//! with `qsort_r`. This port uses `try_reserve` (`ENOMEM`) and a local bottom-up merge sort:
//! `core`'s `sort_by` may panic on an inconsistent comparator, this sort cannot (it never indexes
//! out of bounds and every loop is bounded by increasing indices) — `RD-new: tolerant merge sort`.
//! Cancellation and `errno` save/restore are the shell's.

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{ENOMEM, EOVERFLOW};

use crate::DirSys;
use crate::dirent::{DirEntry, DirStream};

/// The C `INT_MAX` count cap.
pub const INT_MAX_ENTRIES: usize = i32::MAX as usize;

/// The `scandir` comparator: a pointer-to-pointer comparison in C, a reference comparison here.
pub type Compare<'a> = &'a mut dyn FnMut(&DirEntry, &DirEntry) -> i32;
/// The `scandir` selection predicate.
pub type Select<'a> = &'a mut dyn FnMut(&DirEntry) -> bool;

/// Read `path`, keep the entries `sel` accepts, and sort them with `cmp`.
///
/// # Errors
///
/// The backend's error, `EOVERFLOW` past `INT_MAX` entries, or `ENOMEM`.
pub fn scandir<S: DirSys>(
    sys: &mut S,
    path: &[u8],
    sel: Option<Select<'_>>,
    cmp: Option<Compare<'_>>,
) -> Result<Vec<DirEntry>, Errno> {
    scandir_capped(sys, path, sel, cmp, INT_MAX_ENTRIES)
}

/// [`scandir`] with an explicit entry cap, for the `EOVERFLOW` test.
#[doc(hidden)]
pub fn scandir_capped<S: DirSys>(
    sys: &mut S,
    path: &[u8],
    mut sel: Option<Select<'_>>,
    cmp: Option<Compare<'_>>,
    limit: usize,
) -> Result<Vec<DirEntry>, Errno> {
    let mut dir = DirStream::opendir(sys, path)?;
    let mut entries: Vec<DirEntry> = Vec::new();
    loop {
        let entry = match dir.readdir(sys) {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) => {
                let _ = dir.closedir(sys);
                return Err(error);
            }
        };
        if let Some(select) = sel.as_deref_mut()
            && !select(&entry)
        {
            continue;
        }
        if entries.len() >= limit {
            let _ = dir.closedir(sys);
            return Err(Errno(EOVERFLOW as u16));
        }
        if entries.try_reserve(1).is_err() {
            let _ = dir.closedir(sys);
            return Err(Errno(ENOMEM as u16));
        }
        entries.push(entry);
    }
    let _ = dir.closedir(sys);
    if let Some(compare) = cmp {
        crate::sort::merge_sort(&mut entries, compare);
    }
    Ok(entries)
}
