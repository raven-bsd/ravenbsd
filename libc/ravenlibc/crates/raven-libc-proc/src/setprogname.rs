// SPDX-License-Identifier: ISC
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/setprogname.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// Copyright (c) 2013 Antoine Jacoutot <ajacoutot@openbsd.org>
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

//! The pure part of `setprogname`.

use crate::startup::split_progname;

/// Return the basename part of `progname`, without copying.
#[must_use]
pub fn setprogname(progname: &[u8]) -> &[u8] {
    let offset = split_progname(progname);
    progname.get(offset..).unwrap_or(&[])
}

#[cfg(test)]
#[path = "setprogname_ut.rs"]
mod tests;
