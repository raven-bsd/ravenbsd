// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! JSON string escaping and one-object-per-line framing.

use alloc::string::String;
use core::fmt::{self, Write};

use crate::error::ErrorEnvelope;
use crate::record::Record;

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Append the JSON string body of `text` to `out`, without the surrounding quotes.
///
/// The quote, reverse solidus and the short control escapes are written literally; the other
/// control bytes and `DEL` use a lowercase `\u00xx` escape. Valid UTF-8 passes through unchanged.
pub fn escape_json(text: &str, out: &mut String) {
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 || (control as u32) == 0x7f => {
                let value = control as usize;
                out.push_str("\\u00");
                out.push(char::from(HEX[value >> 4]));
                out.push(char::from(HEX[value & 0x0f]));
            }
            other => out.push(other),
        }
    }
}

/// Return the escaped JSON string body of `text`.
#[must_use]
pub fn escaped(text: &str) -> String {
    let mut out = String::new();
    escape_json(text, &mut out);
    out
}

/// Write [`Record`] objects as NDJSON, one object per line, with no wrapper array.
#[derive(Clone, Debug)]
pub struct JsonWriter<W: Write> {
    output: W,
}

impl<W: Write> JsonWriter<W> {
    /// Wrap one output sink.
    pub fn new(output: W) -> Self {
        Self { output }
    }

    /// Write one record and its terminating newline.
    pub fn write_record(&mut self, record: &Record) -> fmt::Result {
        self.output.write_str(&record.to_json())?;
        self.output.write_char('\n')
    }

    /// Write one error envelope and its terminating newline.
    pub fn write_error(&mut self, error: &ErrorEnvelope) -> fmt::Result {
        self.output.write_str(&error.to_json())?;
        self.output.write_char('\n')
    }

    /// Borrow the underlying sink.
    pub fn get_ref(&self) -> &W {
        &self.output
    }

    /// Return the underlying sink.
    pub fn into_inner(self) -> W {
        self.output
    }
}
