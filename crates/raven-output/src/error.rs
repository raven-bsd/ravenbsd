// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The JSON error object written to stderr.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::SCHEMA;
use crate::json::escape_json;

/// One `kind: "error"` object with a stable `code`, a human message and ordered context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorEnvelope {
    code: String,
    message: String,
    context: Vec<(String, String)>,
}

impl ErrorEnvelope {
    /// Start an error with a stable short code and a human message.
    #[must_use]
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: String::from(code),
            message: String::from(message),
            context: Vec::new(),
        }
    }

    /// Add one context pair; pairs keep insertion order.
    #[must_use]
    pub fn with_context(mut self, key: &str, value: &str) -> Self {
        self.context.push((String::from(key), String::from(value)));
        self
    }

    /// The stable code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The human message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The ordered context pairs.
    #[must_use]
    pub fn context(&self) -> &[(String, String)] {
        &self.context
    }

    /// Write the object without a trailing newline.
    pub fn write_json(&self, output: &mut String) -> fmt::Result {
        output.push_str("{\"schema\":");
        write!(output, "{SCHEMA}")?;
        output.push_str(",\"kind\":\"error\",\"code\":\"");
        escape_json(&self.code, output);
        output.push_str("\",\"message\":\"");
        escape_json(&self.message, output);
        output.push_str("\",\"context\":{");
        for (index, (key, value)) in self.context.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push('"');
            escape_json(key, output);
            output.push_str("\":\"");
            escape_json(value, output);
            output.push('"');
        }
        output.push_str("}}");
        Ok(())
    }

    /// The object as one JSON string, without a newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut output = String::new();
        let _ = self.write_json(&mut output);
        output
    }

    /// Write the object and its terminating newline.
    pub fn write_line<W: Write>(&self, output: &mut W) -> fmt::Result {
        output.write_str(&self.to_json())?;
        output.write_char('\n')
    }
}
