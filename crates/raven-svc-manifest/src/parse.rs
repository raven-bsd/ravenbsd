// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared line scanning and value parsing for the manifest subset.

use alloc::string::String;
use alloc::vec::Vec;

use crate::manifest::{ParseError, ParseErrorKind};

/// Maximum manifest/override text size in bytes.
pub(crate) const MAX_TEXT_BYTES: usize = 16 * 1024;
/// Maximum number of entries in one array.
pub(crate) const MAX_ARRAY_ENTRIES: usize = 64;
/// Maximum decoded length of one string in bytes.
pub(crate) const MAX_STRING_BYTES: usize = 256;
/// Maximum number of services in one graph.
pub(crate) const MAX_SERVICES: usize = 256;

/// Build a parse error at `line`.
pub(crate) fn error(line: usize, kind: ParseErrorKind) -> ParseError {
    ParseError { line, kind }
}

/// Remove a `#` comment that is not inside a basic string.
pub(crate) fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == '#' {
            return &line[..index];
        }
    }
    line
}

/// Split `key = value` on the first `=`, trimming both sides.
pub(crate) fn split_key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// True for a bare manifest key or environment key.
pub(crate) fn is_key_text(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// True for a service name: `[a-z0-9_-]{1,32}`.
pub(crate) fn is_service_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

/// True for a scheme name used after `scheme:` or in `namespace`.
pub(crate) fn is_scheme_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        })
}

/// Return the name part of a `scheme:<name>` reference.
pub(crate) fn scheme_name(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("scheme:")
        .filter(|name| is_scheme_name(name))
}

/// True for an environment variable name in the `[env]` table.
pub(crate) fn is_env_key(key: &str) -> bool {
    if key.is_empty() || key.len() > MAX_STRING_BYTES {
        return false;
    }
    let mut bytes = key.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Parse one basic string and require the rest of the line to be empty.
pub(crate) fn string_value(
    text: &str,
    line: usize,
    key: &'static str,
) -> Result<String, ParseError> {
    let mut parser = ValueParser::new(text, line);
    let value = parser.parse_string(key)?;
    parser.finish()?;
    Ok(value)
}

/// Parse one single-line array of basic strings.
pub(crate) fn array_value(
    text: &str,
    line: usize,
    key: &'static str,
) -> Result<Vec<String>, ParseError> {
    let mut parser = ValueParser::new(text, line);
    let value = parser.parse_array(key)?;
    parser.finish()?;
    Ok(value)
}

/// Parse one decimal integer with optional `_` separators.
pub(crate) fn integer_value(text: &str, line: usize, key: &'static str) -> Result<u64, ParseError> {
    let mut parser = ValueParser::new(text, line);
    let value = parser.parse_integer(key)?;
    parser.finish()?;
    Ok(value)
}

/// Parse one `true` or `false` value.
pub(crate) fn bool_value(text: &str, line: usize, key: &'static str) -> Result<bool, ParseError> {
    let mut parser = ValueParser::new(text, line);
    let value = parser.parse_bool(key)?;
    parser.finish()?;
    Ok(value)
}

/// Cursor over one value on one line.
struct ValueParser<'a> {
    text: &'a str,
    position: usize,
    line: usize,
}

impl<'a> ValueParser<'a> {
    fn new(text: &'a str, line: usize) -> Self {
        Self {
            text,
            position: 0,
            line,
        }
    }

    fn error(&self, kind: ParseErrorKind) -> ParseError {
        error(self.line, kind)
    }

    fn rest(&self) -> &'a str {
        &self.text[self.position..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.position += character.len_utf8();
        Some(character)
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.position += 1;
        }
    }

    fn finish(&self) -> Result<(), ParseError> {
        if self.rest().trim().is_empty() {
            Ok(())
        } else {
            Err(self.error(ParseErrorKind::Syntax))
        }
    }

    fn parse_string(&mut self, key: &'static str) -> Result<String, ParseError> {
        if self.bump() != Some('"') {
            return Err(self.error(ParseErrorKind::InvalidValue(key)));
        }
        let mut value = String::new();
        loop {
            let Some(character) = self.bump() else {
                return Err(self.error(ParseErrorKind::Syntax));
            };
            match character {
                '"' => break,
                '\\' => {
                    let Some(escaped) = self.bump() else {
                        return Err(self.error(ParseErrorKind::Syntax));
                    };
                    let decoded = match escaped {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        't' => '\t',
                        _ => return Err(self.error(ParseErrorKind::Syntax)),
                    };
                    value.push(decoded);
                }
                _ => value.push(character),
            }
            if value.len() > MAX_STRING_BYTES {
                return Err(self.error(ParseErrorKind::TooLarge));
            }
        }
        Ok(value)
    }

    fn parse_array(&mut self, key: &'static str) -> Result<Vec<String>, ParseError> {
        if self.bump() != Some('[') {
            return Err(self.error(ParseErrorKind::InvalidValue(key)));
        }
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(values);
        }
        loop {
            self.skip_whitespace();
            if values.len() >= MAX_ARRAY_ENTRIES {
                return Err(self.error(ParseErrorKind::TooLarge));
            }
            values.push(self.parse_string(key)?);
            self.skip_whitespace();
            match self.bump() {
                Some(',') => {
                    self.skip_whitespace();
                    if self.peek() == Some(']') {
                        return Err(self.error(ParseErrorKind::Syntax));
                    }
                }
                Some(']') => break,
                _ => return Err(self.error(ParseErrorKind::Syntax)),
            }
        }
        Ok(values)
    }

    fn parse_integer(&mut self, key: &'static str) -> Result<u64, ParseError> {
        let start = self.position;
        while matches!(self.peek(), Some(character) if character.is_ascii_digit() || character == '_')
        {
            self.bump();
        }
        let raw = &self.text[start..self.position];
        if raw.is_empty() {
            return Err(self.error(ParseErrorKind::InvalidValue(key)));
        }
        if raw.starts_with('_') || raw.ends_with('_') || raw.contains("__") {
            return Err(self.error(ParseErrorKind::Syntax));
        }
        let mut digits = String::with_capacity(raw.len());
        for character in raw.chars() {
            if character == '_' {
                continue;
            }
            if !character.is_ascii_digit() {
                return Err(self.error(ParseErrorKind::Syntax));
            }
            digits.push(character);
        }
        match digits.parse::<u64>() {
            Ok(value) => Ok(value),
            Err(_) => Err(self.error(ParseErrorKind::IntegerOutOfRange(key))),
        }
    }

    fn parse_bool(&mut self, key: &'static str) -> Result<bool, ParseError> {
        let start = self.position;
        while matches!(self.peek(), Some(character) if character.is_ascii_alphabetic()) {
            self.bump();
        }
        match &self.text[start..self.position] {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(self.error(ParseErrorKind::InvalidValue(key))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_respect_strings() {
        assert_eq!(strip_comment("a = \"x#y\" # z"), "a = \"x#y\" ");
        assert_eq!(strip_comment("a = \"x\\\"#y\""), "a = \"x\\\"#y\"");
        assert_eq!(strip_comment("plain # comment"), "plain ");
    }

    #[test]
    fn split_requires_both_sides() {
        assert_eq!(split_key_value("a = \"b\""), Some(("a", "\"b\"")));
        assert_eq!(split_key_value("a ="), None);
        assert_eq!(split_key_value("= b"), None);
    }

    #[test]
    fn value_types_are_strict() {
        assert!(string_value("\"x\"", 1, "k").is_ok());
        assert!(matches!(
            string_value("1", 1, "k").unwrap_err().kind,
            ParseErrorKind::InvalidValue("k")
        ));
        assert!(matches!(
            string_value("\"x\" junk", 1, "k").unwrap_err().kind,
            ParseErrorKind::Syntax
        ));
        assert!(array_value("[]", 1, "k").unwrap().is_empty());
        assert!(matches!(
            array_value("[\"a\",]", 1, "k").unwrap_err().kind,
            ParseErrorKind::Syntax
        ));
        assert_eq!(integer_value("1_500", 1, "k").unwrap(), 1500);
        assert!(matches!(
            integer_value("_15", 1, "k").unwrap_err().kind,
            ParseErrorKind::Syntax
        ));
        assert!(bool_value("true", 1, "k").unwrap());
        assert!(matches!(
            bool_value("yes", 1, "k").unwrap_err().kind,
            ParseErrorKind::InvalidValue("k")
        ));
    }

    #[test]
    fn string_limits_and_escapes() {
        let escaped = string_value("\"a\\\"b\\\\c\\nd\\te\"", 1, "k").unwrap();
        assert_eq!(escaped, "a\"b\\c\nd\te");
        let long = "x".repeat(MAX_STRING_BYTES + 1);
        assert!(matches!(
            string_value(&alloc::format!("\"{long}\""), 1, "k")
                .unwrap_err()
                .kind,
            ParseErrorKind::TooLarge
        ));
        assert!(matches!(
            string_value("\"\\u1234\"", 1, "k").unwrap_err().kind,
            ParseErrorKind::Syntax
        ));
    }
}
