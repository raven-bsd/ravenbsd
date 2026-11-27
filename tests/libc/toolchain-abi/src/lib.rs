// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Host-side comparison of the `ravenbsd` module in the toolchain `libc`
//! crate with the ravenlibc ABI generated from `libc/ravenlibc/abi`.
//!
//! The toolchain module is not compiled on the host (it is selected only for
//! `target_os = "ravenbsd"`), so this crate reads the committed Rust source
//! and the generated ABI source. It fails on a missing or different constant,
//! on a wrong scalar width/signedness, or on a changed `stat`/`statvfs`/
//! `termios` field layout.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

/// The vendored toolchain libc RavenBSD module.
pub const TOOLCHAIN_LIBC: &str =
    include_str!("../../../../toolchain/vendor/libc/src/unix/ravenbsd/mod.rs");
/// The canonical fcntl/AT/F constants.
pub const ABI_FCNTL: &str =
    include_str!("../../../../libc/ravenlibc/crates/raven-libc-abi/src/generated/fcntl.rs");
/// The canonical errno constants.
pub const ABI_ERRNO: &str =
    include_str!("../../../../libc/ravenlibc/crates/raven-libc-abi/src/generated/errno.rs");
/// The canonical socket constants.
pub const ABI_SOCKET: &str =
    include_str!("../../../../libc/ravenlibc/crates/raven-libc-abi/src/generated/socket.rs");
/// The canonical netinet constants.
pub const ABI_NETINET: &str =
    include_str!("../../../../libc/ravenlibc/crates/raven-libc-abi/src/generated/netinet.rs");
/// The C `termios` header; it is the source of the terminal constants that are
/// not in the generated ABI group files.
pub const TERMIOS_HEADER: &str = include_str!("../../../../libc/ravenlibc/include/bits/termios.h");

/// Remove Rust/C comments before parsing declarations.
#[must_use]
pub fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            let _ = chars.next();
            let mut previous = '\0';
            for next in chars.by_ref() {
                if previous == '*' && next == '/' {
                    break;
                }
                previous = next;
            }
        } else if c == '/' && chars.peek() == Some(&'/') {
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse `pub const NAME: type = EXPR;` declarations.
#[must_use]
pub fn parse_constants(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in strip_comments(text).lines() {
        let Some(start) = line.find("pub const ") else {
            continue;
        };
        let rest = &line[start + "pub const ".len()..];
        let Some((name, after)) = rest.split_once(':') else {
            continue;
        };
        let Some((_, expression)) = after.split_once('=') else {
            continue;
        };
        let expression = expression.split(';').next().unwrap_or("").trim();
        if !expression.is_empty() {
            out.insert(name.trim().to_string(), expression.to_string());
        }
    }
    out
}

/// Parse `#define NAME VALUE` lines from a C header.
#[must_use]
pub fn parse_defines(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in strip_comments(text).lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut fields = rest.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(value) = fields.next() else {
            continue;
        };
        if value.starts_with('(') {
            continue;
        }
        out.insert(name.to_string(), value.to_string());
    }
    out
}

/// Parse `pub type NAME = RHS;` declarations.
#[must_use]
pub fn parse_types(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in strip_comments(text).lines() {
        let Some(start) = line.find("pub type ") else {
            continue;
        };
        let rest = &line[start + "pub type ".len()..];
        let Some((name, after)) = rest.split_once('=') else {
            continue;
        };
        let rhs = after.split(';').next().unwrap_or("").trim();
        if !rhs.is_empty() {
            out.insert(name.trim().to_string(), rhs.to_string());
        }
    }
    out
}

/// Parse the named field list of `pub struct <name> { ... }`.
#[must_use]
pub fn parse_struct_fields(text: &str, name: &str) -> Option<Vec<(String, String)>> {
    let needle = format!("pub struct {name} {{");
    let start = text.find(&needle)? + needle.len();
    let mut depth = 1usize;
    let mut end = start;
    for (offset, c) in text[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &text[start..end];
    let mut fields = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("#[") || line.starts_with("//") {
            continue;
        }
        let line = line.strip_prefix("pub ").unwrap_or(line);
        let Some((field, after)) = line.split_once(':') else {
            continue;
        };
        let field = field.trim().to_string();
        if field.is_empty() || field.starts_with('}') {
            continue;
        }
        let ty = after.split(',').next().unwrap_or("").trim().to_string();
        if !ty.is_empty() {
            fields.push((field, ty));
        }
    }
    Some(fields)
}

enum Token {
    Number(i128),
    Name(String),
    Operator(char),
    LeftParen,
    RightParen,
}

fn tokenize(expression: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = expression.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            let _ = chars.next();
        } else if c == '(' {
            let _ = chars.next();
            tokens.push(Token::LeftParen);
        } else if c == ')' {
            let _ = chars.next();
            tokens.push(Token::RightParen);
        } else if c.is_ascii_digit() {
            let mut raw = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    raw.push(next);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(parse_number(&raw)?));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut raw = String::new();
            while let Some(&next) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    raw.push(next);
                    let _ = chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Name(raw));
        } else if "|^&<>+-!".contains(c) {
            let _ = chars.next();
            tokens.push(Token::Operator(c));
        } else {
            return None;
        }
    }
    Some(tokens)
}

fn parse_number(raw: &str) -> Option<i128> {
    let text = raw.replace('_', "");
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return i128::from_str_radix(hex, 16).ok();
    }
    if let Some(octal) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        return i128::from_str_radix(octal, 8).ok();
    }
    if let Some(binary) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        return i128::from_str_radix(binary, 2).ok();
    }
    if text.len() > 1 && text.starts_with('0') && text.chars().all(|c| ('0'..='7').contains(&c)) {
        return i128::from_str_radix(&text[1..], 8).ok();
    }
    text.parse().ok()
}

struct Parser<'a> {
    tokens: Vec<Token>,
    position: usize,
    definitions: &'a BTreeMap<String, String>,
    depth: usize,
}

impl Parser<'_> {
    fn parse_or(&mut self) -> Option<i128> {
        let mut value = self.parse_xor()?;
        while self.take_operator('|') {
            value |= self.parse_xor()?;
        }
        Some(value)
    }

    fn parse_xor(&mut self) -> Option<i128> {
        let mut value = self.parse_and()?;
        while self.take_operator('^') {
            value ^= self.parse_and()?;
        }
        Some(value)
    }

    fn parse_and(&mut self) -> Option<i128> {
        let mut value = self.parse_shift()?;
        while self.take_operator('&') {
            value &= self.parse_shift()?;
        }
        Some(value)
    }

    fn parse_shift(&mut self) -> Option<i128> {
        let mut value = self.parse_add()?;
        loop {
            if self.take_operator('<') {
                if !self.take_operator('<') {
                    return None;
                }
                value = value.checked_shl(u32::try_from(self.parse_add()?).ok()?)?;
            } else if self.take_operator('>') {
                if !self.take_operator('>') {
                    return None;
                }
                value = value.checked_shr(u32::try_from(self.parse_add()?).ok()?)?;
            } else {
                break;
            }
        }
        Some(value)
    }

    fn parse_add(&mut self) -> Option<i128> {
        let mut value = self.parse_unary()?;
        loop {
            if self.take_operator('+') {
                value = value.checked_add(self.parse_unary()?)?;
            } else if self.take_operator('-') {
                value = value.checked_sub(self.parse_unary()?)?;
            } else {
                break;
            }
        }
        Some(value)
    }

    fn parse_unary(&mut self) -> Option<i128> {
        if self.take_operator('-') {
            return self.parse_unary()?.checked_neg();
        }
        if self.take_operator('!') {
            return Some(!self.parse_unary()?);
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<i128> {
        match self.tokens.get(self.position)? {
            Token::Number(value) => {
                self.position += 1;
                Some(*value)
            }
            Token::Name(name) => {
                self.position += 1;
                let expression = self.definitions.get(name)?;
                if self.depth > 64 {
                    return None;
                }
                eval_depth(expression, self.definitions, self.depth + 1)
            }
            Token::LeftParen => {
                self.position += 1;
                let value = self.parse_or()?;
                if !matches!(self.tokens.get(self.position), Some(Token::RightParen)) {
                    return None;
                }
                self.position += 1;
                Some(value)
            }
            Token::Operator(_) | Token::RightParen => None,
        }
    }

    fn take_operator(&mut self, wanted: char) -> bool {
        if matches!(self.tokens.get(self.position), Some(Token::Operator(c)) if *c == wanted) {
            self.position += 1;
            true
        } else {
            false
        }
    }
}

fn eval_depth(
    expression: &str,
    definitions: &BTreeMap<String, String>,
    depth: usize,
) -> Option<i128> {
    let tokens = tokenize(expression)?;
    let mut parser = Parser {
        tokens,
        position: 0,
        definitions,
        depth,
    };
    let value = parser.parse_or()?;
    (parser.position == parser.tokens.len()).then_some(value)
}

/// Evaluate every constant in a declaration map.
#[must_use]
pub fn evaluate(text: &str) -> BTreeMap<String, i128> {
    let definitions = parse_constants(text);
    definitions
        .keys()
        .filter_map(|name| {
            eval_depth(&definitions[name], &definitions, 0).map(|value| (name.clone(), value))
        })
        .collect()
}

/// Evaluate the C header defines (the same expressions, C octal literals).
#[must_use]
pub fn evaluate_defines(text: &str) -> BTreeMap<String, i128> {
    let definitions = parse_defines(text);
    definitions
        .keys()
        .filter_map(|name| {
            eval_depth(&definitions[name], &definitions, 0).map(|value| (name.clone(), value))
        })
        .collect()
}

/// Compare every ABI name with `prefix` against the toolchain module.
#[must_use]
pub fn compare_prefix(prefix: &str, abi: &str, toolchain: &str) -> Vec<String> {
    let abi = evaluate(abi);
    let toolchain = evaluate(toolchain);
    let mut findings = Vec::new();
    for (name, expected) in abi {
        if !name.starts_with(prefix) {
            continue;
        }
        match toolchain.get(&name) {
            Some(actual) if actual == &expected => {}
            Some(actual) => findings.push(format!("{name}: toolchain {actual}, ABI {expected}")),
            None => findings.push(format!("{name}: missing from the toolchain libc")),
        }
    }
    findings
}

/// Compare a required subset of ABI names.
#[must_use]
pub fn compare_required(names: &[&str], abi: &str, toolchain: &str) -> Vec<String> {
    let abi = evaluate(abi);
    let toolchain = evaluate(toolchain);
    let mut findings = Vec::new();
    for name in names {
        let expected = abi.get(*name);
        let actual = toolchain.get(*name);
        if expected.is_none() {
            findings.push(format!("{name}: missing from the ABI"));
            continue;
        }
        if actual.is_none() {
            findings.push(format!("{name}: missing from the toolchain libc"));
            continue;
        }
        if expected != actual {
            findings.push(format!(
                "{name}: toolchain {}, ABI {}",
                actual.map_or_else(|| "?".to_string(), ToString::to_string),
                expected.map_or_else(|| "?".to_string(), ToString::to_string)
            ));
        }
    }
    findings
}

/// Compare the terminal constants in the toolchain module with the header.
#[must_use]
pub fn compare_termios(names: &[&str]) -> Vec<String> {
    let header = evaluate_defines(TERMIOS_HEADER);
    let toolchain = evaluate(TOOLCHAIN_LIBC);
    let mut findings = Vec::new();
    for name in names {
        let Some(expected) = header.get(*name) else {
            findings.push(format!("{name}: missing from bits/termios.h"));
            continue;
        };
        match toolchain.get(*name) {
            Some(actual) if actual == expected => {}
            Some(actual) => findings.push(format!("{name}: toolchain {actual}, header {expected}")),
            None => findings.push(format!("{name}: missing from the toolchain libc")),
        }
    }
    findings
}

/// The width and signedness of a Rust scalar type used in the module.
#[must_use]
pub fn scalar_shape(name: &str) -> Option<(u32, bool)> {
    Some(match name {
        "i8" | "c_char" => (8, true),
        "u8" | "c_uchar" | "cc_t" => (8, false),
        "i16" => (16, true),
        "u16" => (16, false),
        "i32" | "c_int" | "blksize_t" => (32, true),
        "u32" | "c_uint" | "mode_t" | "uid_t" | "gid_t" | "nlink_t" | "tcflag_t" | "speed_t" => {
            (32, false)
        }
        "i64" | "c_long" | "c_longlong" | "off_t" | "time_t" | "suseconds_t" | "clock_t"
        | "blkcnt_t" => (64, true),
        "u64" | "c_ulong" | "c_ulonglong" | "dev_t" | "ino_t" | "fsblkcnt_t" | "fsfilcnt_t" => {
            (64, false)
        }
        "usize" => (64, false),
        "isize" => (64, true),
        _ => return None,
    })
}

/// Compare the named type aliases to `(width, signed)` expectations.
#[must_use]
pub fn compare_types(expected: &[(&str, (u32, bool))]) -> Vec<String> {
    let types = parse_types(TOOLCHAIN_LIBC);
    let mut findings = Vec::new();
    for (name, wanted) in expected {
        let Some(rhs) = types.get(*name) else {
            findings.push(format!("{name}: missing from the toolchain libc"));
            continue;
        };
        let shape = scalar_shape(rhs.trim_start_matches("crate::"));
        if shape != Some(*wanted) {
            findings.push(format!(
                "{name}: toolchain {rhs} ({shape:?}), expected {wanted:?}"
            ));
        }
    }
    findings
}

fn normalized_type(name: &str) -> Option<String> {
    let name = name.trim_start_matches("crate::");
    if let Some(rest) = name.strip_prefix('[') {
        let (element, count) = rest.split_once(';')?;
        let count = count.trim_end_matches(']').trim();
        let element = normalized_type(element.trim())?;
        let count = count.trim_start_matches("crate::");
        let count = if let Ok(number) = count.parse::<u32>() {
            number
        } else {
            let constants = evaluate(TOOLCHAIN_LIBC);
            u32::try_from(*constants.get(count)?).ok()?
        };
        return Some(format!("[{element}; {count}]"));
    }
    // The vendored module marks donor padding slots with the zero-sized `Padding<T>`
    // shim; the budgeted ABI record names the same slot with its scalar type.
    if let Some(inner) = name
        .strip_prefix("Padding<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return normalized_type(inner.trim());
    }
    let (width, signed) = scalar_shape(name)?;
    Some(match (width, signed) {
        (8, true) => "i8".into(),
        (8, false) => "u8".into(),
        (16, true) => "i16".into(),
        (16, false) => "u16".into(),
        (32, true) => "i32".into(),
        (32, false) => "u32".into(),
        (64, true) => "i64".into(),
        (64, false) => "u64".into(),
        _ => return None,
    })
}

/// Compare a struct's field type sequence with the expected sequence.
#[must_use]
pub fn compare_struct(name: &str, expected: &[&str]) -> Vec<String> {
    let Some(fields) = parse_struct_fields(TOOLCHAIN_LIBC, name) else {
        return vec![format!("{name}: struct not found")];
    };
    let mut findings = Vec::new();
    if fields.len() != expected.len() {
        findings.push(format!(
            "{name}: {} fields, expected {}",
            fields.len(),
            expected.len()
        ));
    }
    for (index, (field, ty)) in fields.iter().enumerate() {
        let Some(wanted) = expected.get(index) else {
            findings.push(format!("{name}.{field}: unexpected field"));
            continue;
        };
        let Some(actual) = normalized_type(ty) else {
            findings.push(format!("{name}.{field}: unsupported type {ty}"));
            continue;
        };
        if actual != *wanted {
            findings.push(format!("{name}.{field}: {actual}, expected {wanted}"));
        }
    }
    findings
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "lib_ut.rs"]
mod tests;
