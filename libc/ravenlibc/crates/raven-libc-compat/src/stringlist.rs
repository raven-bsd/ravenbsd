// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1994 Christos Zoulas
// All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/stringlist.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $NetBSD: stringlist.c,v 1.2 1997/01/17 07:26:20 lukem Exp $)
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS
// OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
// WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stringlist`: `sl_init`, `sl_add`, `sl_free`, `sl_find` (donor-map 15 T3, 02 B11).
//!
//! The C `StringList` is a `char **` with a capacity; the Rust [`StringList`] is the list of
//! owned byte strings. The shell owns the C struct and its `sl_str`/`sl_max`/`sl_cur`
//! bookkeeping. The donor's `_SL_CHUNKSIZE` growth is irrelevant to `Vec`; the observable
//! `sl_add` failure (`-1`/`ENOMEM`) becomes [`Errno`].

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

/// The C `StringList`: `sl_str`, `sl_max`, `sl_cur`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringList {
    items: Vec<Vec<u8>>,
}

impl StringList {
    /// `sl_init`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `sl_add`: append one name.
    ///
    /// The C version returns `-1` when `reallocf` fails; `Vec` allocation failure aborts in
    /// Rust, so the `Result` is always `Ok` and exists to preserve the C signature.
    pub fn add(&mut self, s: Vec<u8>) -> Result<(), Errno> {
        self.items.push(s);
        Ok(())
    }

    /// `sl_find`: the index of `name`, or `None`.
    ///
    /// The donor compares with `strcmp` (the stored string has no embedded NUL in practice);
    /// `case_insensitive` selects the `strcasecmp` variant some callers need.
    #[must_use]
    pub fn find(&self, name: &[u8], case_insensitive: bool) -> Option<usize> {
        self.items.iter().position(|item| {
            if case_insensitive {
                item.eq_ignore_ascii_case(name)
            } else {
                item.as_slice() == name
            }
        })
    }

    /// The stored names (`sl_str[0..sl_cur]`).
    #[must_use]
    pub fn items(&self) -> &[Vec<u8>] {
        &self.items
    }

    /// `sl_free`: drop everything (the C version frees the vector and, with `all`, the strings).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// `sl_cur`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
#[path = "stringlist_ut.rs"]
mod tests;
