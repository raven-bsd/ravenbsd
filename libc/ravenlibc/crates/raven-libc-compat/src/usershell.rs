// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1985, 1993 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/getusershell.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: getusershell.c,v 1.18 2019/12/10 02:35:16 millert Exp $)
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
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getusershell`/`setusershell`/`endusershell` (donor-map 15 T3, 02 X31).
//!
//! The donor's compiled default list (`/bin/sh`, `/bin/csh`) is replaced by the ticket's rule:
//! `/etc/shells` when readable, otherwise the single entry `/bin/sh`. Lines must start with
//! `/`; the text is cut at `#` or the newline.

use alloc::vec::Vec;

use crate::seams::ReadFile;

/// `_PATH_SHELLS`.
pub const PATH_SHELLS: &[u8] = b"/etc/shells";
/// The single fallback shell.
pub const FALLBACK_SHELL: &[u8] = b"/bin/sh";

/// The parsed `/etc/shells` and its cursor.
#[derive(Clone, Debug, Default)]
pub struct Shells {
    entries: Vec<Vec<u8>>,
    cursor: usize,
}

impl Shells {
    /// Load `/etc/shells` through the seam, or the single `/bin/sh` entry.
    #[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the split is bounded by the read buffer
    pub fn load<R: ReadFile>(r: &mut R) -> Self {
        let mut contents = Vec::new();
        if r.read_all(PATH_SHELLS, &mut contents).is_err() {
            return Self {
                entries: alloc::vec![FALLBACK_SHELL.to_vec()],
                cursor: 0,
            };
        }
        let mut entries = Vec::new();
        let mut start = 0usize;
        for (index, &byte) in contents.iter().enumerate() {
            if byte == b'\n' {
                if let Some(shell) = parse_line(&contents[start..index]) {
                    entries.push(shell);
                }
                start = index.wrapping_add(1);
            }
        }
        if start < contents.len()
            && let Some(shell) = parse_line(&contents[start..])
        {
            entries.push(shell);
        }
        if entries.is_empty() {
            entries.push(FALLBACK_SHELL.to_vec());
        }
        Self { entries, cursor: 0 }
    }

    /// `getusershell`: the next shell.
    #[allow(clippy::should_implement_trait)] // the C name is `getusershell`
    pub fn next(&mut self) -> Option<&[u8]> {
        let entry = self.entries.get(self.cursor);
        if entry.is_some() {
            self.cursor = self.cursor.wrapping_add(1);
        }
        entry.map(Vec::as_slice)
    }

    /// `setusershell`.
    pub fn rewind(&mut self) {
        self.cursor = 0;
    }

    /// The loaded list.
    #[must_use]
    pub fn entries(&self) -> &[Vec<u8>] {
        &self.entries
    }
}

fn parse_line(line: &[u8]) -> Option<Vec<u8>> {
    if line.first().copied() != Some(b'/') {
        return None;
    }
    let end = line
        .iter()
        .position(|&c| c == b'#' || c == b'\n')
        .unwrap_or(line.len());
    if end == 0 {
        return None;
    }
    line.get(..end).map(<[u8]>::to_vec)
}

#[cfg(test)]
#[path = "usershell_ut.rs"]
mod tests;
