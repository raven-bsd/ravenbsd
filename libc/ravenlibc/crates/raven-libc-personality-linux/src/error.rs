// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! GNU `error()`/`error_at_line()` formatting (`L09`, `15` T2 row `error`).
//!
//! Order (glibc manual, `15` T2): the message count is incremented, stdout is flushed (the
//! shell), the program name is the hook output or `program_invocation_name`, the formatted
//! message follows, then `": " + strerror(errnum)` **only when `errnum != 0`**, then a newline
//! and `exit(status)` when non-zero. `error_at_line` adds `file:line:` and suppresses a repeat
//! of the same `(file, line)` pair when `error_one_per_line != 0`.
//!
//! The known mlibc bug of printing the strerror text for `errnum == 0` is deliberately not
//! reproduced. No formatting macro is used: the decimal line number is rendered by hand so the
//! crate stays free of `core::fmt` (`16` R-5).

use alloc::vec::Vec;

/// `error_at_line` suppression state and the shared globals of the shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorState {
    /// Number of messages printed (`error_message_count`).
    pub message_count: u32,
    /// `error_one_per_line`; non-zero enables the `(file, line)` suppression.
    pub one_per_line: i32,
    last: Option<(Vec<u8>, u32)>,
}

impl ErrorState {
    /// A fresh state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            message_count: 0,
            one_per_line: 0,
            last: None,
        }
    }

    /// The remembered `(file, line)` pair of the last printed call.
    #[must_use]
    pub fn last(&self) -> Option<(&[u8], u32)> {
        self.last
            .as_ref()
            .map(|(file, line)| (file.as_slice(), *line))
    }
}

/// Append the decimal form of `value` without a formatting engine.
fn append_decimal(out: &mut Vec<u8>, mut value: u32) {
    let mut digits = [0u8; 10];
    let mut used = 0usize;
    if value == 0 {
        out.push(b'0');
        return;
    }
    while value > 0 && used < digits.len() {
        let digit = u8::try_from(value.checked_rem(10).unwrap_or(0)).unwrap_or(0);
        if let Some(slot) = digits.get_mut(used) {
            *slot = b'0'.saturating_add(digit);
        }
        used = used.saturating_add(1);
        value = value.checked_div(10).unwrap_or(0);
    }
    while used > 0 {
        used = used.saturating_sub(1);
        if let Some(byte) = digits.get(used) {
            out.push(*byte);
        }
    }
}

/// Append the program prefix followed by the message, optional strerror and newline.
fn append_message(
    out: &mut Vec<u8>,
    program: &[u8],
    errnum: i32,
    message: &[u8],
    strerror: Option<&[u8]>,
) {
    if !program.is_empty() {
        out.extend_from_slice(program);
        out.extend_from_slice(b": ");
    }
    out.extend_from_slice(message);
    if errnum != 0 {
        out.extend_from_slice(b": ");
        if let Some(text) = strerror {
            out.extend_from_slice(text);
        }
    }
    out.push(b'\n');
}

/// `error(status, errnum, fmt, ...)`: append the rendered line to `out`.
///
/// The shell increments `error_message_count`, flushes stdout and exits for a non-zero status.
pub fn format_error(
    program: &[u8],
    errnum: i32,
    message: &[u8],
    strerror: Option<&[u8]>,
    out: &mut Vec<u8>,
) {
    append_message(out, program, errnum, message, strerror);
}

/// `error_at_line(status, errnum, file, line, fmt, ...)`.
///
/// Returns `true` when a message was appended and `false` when the `(file, line)` pair was
/// suppressed. The count is incremented only for a printed message; the caller still honours
/// `status`.
#[allow(clippy::too_many_arguments)]
pub fn format_error_at_line(
    st: &mut ErrorState,
    program: &[u8],
    errnum: i32,
    file: &[u8],
    line: u32,
    message: &[u8],
    strerror: Option<&[u8]>,
    out: &mut Vec<u8>,
) -> bool {
    if st.one_per_line != 0
        && st.last.as_ref().is_some_and(|(last_file, last_line)| {
            last_file.as_slice() == file && *last_line == line
        })
    {
        return false;
    }
    st.last = Some((file.to_vec(), line));
    st.message_count = st.message_count.saturating_add(1);
    if !program.is_empty() {
        out.extend_from_slice(program);
    }
    out.push(b':');
    out.extend_from_slice(file);
    out.push(b':');
    append_decimal(out, line);
    out.extend_from_slice(b": ");
    out.extend_from_slice(message);
    if errnum != 0 {
        out.extend_from_slice(b": ");
        if let Some(text) = strerror {
            out.extend_from_slice(text);
        }
    }
    out.push(b'\n');
    true
}

#[cfg(test)]
#[path = "error_ut.rs"]
mod tests;
