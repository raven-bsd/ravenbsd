// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One HTTP header field.

use alloc::string::String;

/// One `name: value` header field, stored exactly as sent (the value has surrounding OWS
/// removed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// Field name.
    pub name: String,
    /// Field value without leading or trailing optional whitespace.
    pub value: String,
}

impl Header {
    /// Build a header after validating the token name and the value.
    pub fn new(name: &str, value: &str) -> Option<Self> {
        if !valid_name(name) || !valid_value(value) {
            return None;
        }
        Some(Self {
            name: String::from(name),
            value: String::from(value),
        })
    }

    /// True when the field name matches `name` case-insensitively.
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }
}

/// HTTP `token` characters (RFC 9110 section 5.6.2), excluding separators and controls.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'%'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            ) || byte.is_ascii_alphanumeric()
        })
}

/// A field value may contain visible ASCII and horizontal tab, but never CR, LF or another
/// control byte. This is the request-smuggling boundary.
pub(crate) fn valid_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte == b'\t' || (0x20..=0x7e).contains(&byte))
}

#[cfg(test)]
#[path = "header_ut.rs"]
mod tests;
