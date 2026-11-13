// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/string/strsignal.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header; covered by musl/COPYRIGHT)
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

//! `strsignal`: one NUL-separated blob plus `u16` offsets.
//!
//! RavenBSD: the donor's `sigmap` branch is dead under Linux numbering (DR-13), so only the
//! identity path is translated. Valid numbers are `1..=64` (`_NSIG` 65, H-19); everything else
//! uses the `"Unknown signal: N"` buffer path.

use super::BUF_LEN;
use super::strerror::write_unknown_prefixed;

/// Signal descriptions 1..=64, including `RT32`..`RT64`.
const SIGNAL_BLOB: &[u8] = b"Hangup\x00Interrupt\x00Quit\x00Illegal instruction\x00Trace/breakpoint trap\x00Aborted\x00Bus error\x00Arithmetic exception\x00Killed\x00User defined signal 1\x00Segmentation fault\x00User defined signal 2\x00Broken pipe\x00Alarm clock\x00Terminated\x00Stack fault\x00Child process status\x00Continued\x00Stopped (signal)\x00Stopped\x00Stopped (tty input)\x00Stopped (tty output)\x00Urgent I/O condition\x00CPU time limit exceeded\x00File size limit exceeded\x00Virtual timer expired\x00Profiling timer expired\x00Window changed\x00I/O possible\x00Power failure\x00Bad system call\x00RT32\x00RT33\x00RT34\x00RT35\x00RT36\x00RT37\x00RT38\x00RT39\x00RT40\x00RT41\x00RT42\x00RT43\x00RT44\x00RT45\x00RT46\x00RT47\x00RT48\x00RT49\x00RT50\x00RT51\x00RT52\x00RT53\x00RT54\x00RT55\x00RT56\x00RT57\x00RT58\x00RT59\x00RT60\x00RT61\x00RT62\x00RT63\x00RT64\x00";

const SIGNAL_COUNT: usize = 64;

/// Byte offsets of signals 1..=64 in [`SIGNAL_BLOB`].
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
const SIGNAL_OFF: [u16; SIGNAL_COUNT] = {
    let mut offsets = [0u16; SIGNAL_COUNT];
    let mut position = 0usize;
    let mut index = 0usize;
    while index < SIGNAL_COUNT {
        offsets[index] = position as u16;
        while position < SIGNAL_BLOB.len() && SIGNAL_BLOB[position] != 0 {
            position += 1;
        }
        position += 1;
        index += 1;
    }
    offsets
};

/// Static description of signal number `s` (1-based), without a NUL.
#[must_use]
pub fn signal_message(s: i32) -> Option<&'static [u8]> {
    if s < 1 || s > SIGNAL_COUNT as i32 {
        return None;
    }
    let index = (s as usize).wrapping_sub(1);
    let start = usize::from(*SIGNAL_OFF.get(index)?);
    let rest = SIGNAL_BLOB.get(start..)?;
    let mut end = 0usize;
    loop {
        if *rest.get(end)? == 0 {
            break;
        }
        end = end.wrapping_add(1);
    }
    rest.get(..end)
}

/// Description of `s` into the caller's 64-byte buffer: `(text without NUL, known)`.
///
/// Unknown numbers (0, negative, `>= 65`) get `"Unknown signal: N"`.
#[must_use]
pub fn strsignal_into(s: i32, buf: &mut [u8; BUF_LEN]) -> (&[u8], bool) {
    if let Some(text) = signal_message(s) {
        return (text, true);
    }
    let written = write_unknown_prefixed(buf, b"Unknown signal:", s);
    (buf.get(..written).unwrap_or(&[]), false)
}

#[cfg(test)]
#[path = "strsignal_ut.rs"]
mod tests;
