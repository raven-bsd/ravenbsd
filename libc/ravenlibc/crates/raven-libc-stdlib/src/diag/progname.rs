// SPDX-License-Identifier: ISC
// Copyright (c) 2013 Antoine Jacoutot <ajacoutot@openbsd.org>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/setprogname.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: setprogname.c,v 1.6 2017/09/17 06:38:03 otto Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

//! The program-name store; `getprogname.c` is the one-line accessor and is not translated as a
//! separate file.

/// The `__progname` value the shell keeps in `.bss`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct ProgName {
    name: &'static [u8],
}

impl ProgName {
    /// An empty name until `setprogname` runs.
    #[must_use]
    pub const fn new() -> Self {
        Self { name: b"" }
    }

    /// `setprogname(progname)`: keep the part after the last `/`.
    pub fn set(&mut self, progname: &'static [u8]) {
        self.name = match progname.iter().rposition(|byte| *byte == b'/') {
            Some(slash) => progname.get(slash.wrapping_add(1)..).unwrap_or(b""),
            None => progname,
        };
    }

    /// The current name, empty until set.
    #[must_use]
    pub fn get(&self) -> &'static [u8] {
        self.name
    }

    /// The name for diagnostics, `?` when unset.
    #[must_use]
    pub fn display(&self) -> &'static [u8] {
        if self.name.is_empty() {
            b"?"
        } else {
            self.name
        }
    }
}

impl Default for ProgName {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "progname_ut.rs"]
mod tests;
