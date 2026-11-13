// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The shared `verr`/`vwarn` writer: program name, user message and `strerror` order.
//!
//! The donor writes `"<progname>: "`, then the `vfprintf` of the user format, then `": "`, then
//! `strerror(code)`, then `"\n"`. `user_message` is the shell running `vfprintf` on its own
//! `va_list`; this module only sequences the writes and never formats.

use raven_libc_str::errmsg::strerror_into;

/// The shell's sink for `err`/`warn`.
pub trait DiagOut {
    /// Write bytes to stderr.
    fn write(&mut self, b: &[u8]);
    /// Run the caller's `vfprintf(stderr, fmt, ap)`.
    fn user_message(&mut self);
}

/// The message text buffer for `strerror` (RD-13's 64-byte buffer).
const TEXT_LEN: usize = 64;

/// Write `<prog>: [<message>: ]<strerror>\n`; returns `eval` for the `err` forms.
pub fn warn_common<D: DiagOut>(
    out: &mut D,
    prog: &[u8],
    code: i32,
    has_message: bool,
    eval: Option<i32>,
) -> i32 {
    out.write(prog);
    out.write(b": ");
    if has_message {
        out.user_message();
        out.write(b": ");
    }
    let mut buffer = [0u8; TEXT_LEN];
    let (text, _known) = strerror_into(code, &mut buffer);
    out.write(text);
    out.write(b"\n");
    eval.unwrap_or(0)
}

/// Write `<prog>: [<message>: ]<explicit text>\n` for the `x` forms.
pub fn warn_explicit<D: DiagOut>(
    out: &mut D,
    prog: &[u8],
    has_message: bool,
    eval: Option<i32>,
) -> i32 {
    out.write(prog);
    out.write(b": ");
    if has_message {
        out.user_message();
    }
    out.write(b"\n");
    eval.unwrap_or(0)
}

#[cfg(test)]
#[path = "err_core_ut.rs"]
mod tests;
