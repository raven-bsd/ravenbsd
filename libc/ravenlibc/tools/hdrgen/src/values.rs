// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer literals, constant evaluation and the C/Rust renderings of one entry.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::model::{ConstDef, Term};

/// Parse a TOML/C integer literal: decimal, `0x`, `0o`, `0b`, optional sign and C suffix.
pub fn parse_int_literal(text: &str) -> Result<i128> {
    let trimmed = text.trim();
    let (negative, body) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (
            false,
            trimmed.strip_prefix('+').map_or(trimmed, str::trim_start),
        ),
    };
    let (radix, digits) =
        if let Some(rest) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
            (16, rest)
        } else if let Some(rest) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
            (8, rest)
        } else if let Some(rest) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
            (2, rest)
        } else {
            (10, body)
        };
    let digits = strip_suffix(digits);
    if digits.is_empty() {
        bail!("empty integer literal {text:?}");
    }
    let value = i128::from_str_radix(digits, radix)
        .map_err(|error| anyhow::anyhow!("invalid integer literal {text:?}: {error}"))?;
    Ok(if negative { -value } else { value })
}

fn strip_suffix(digits: &str) -> &str {
    let end = digits
        .find(|character: char| !character.is_ascii_hexdigit())
        .unwrap_or(digits.len());
    let (head, tail) = digits.split_at(end);
    if tail
        .chars()
        .all(|character| matches!(character, 'u' | 'U' | 'l' | 'L'))
    {
        head
    } else {
        digits
    }
}

/// Evaluate a definition, resolving aliases and terms through `resolve`.
pub fn evaluate(def: &ConstDef, resolve: &mut dyn FnMut(&str) -> Result<i128>) -> Result<i128> {
    let components = usize::from(def.value.is_some())
        + usize::from(def.alias.is_some())
        + usize::from(def.or.is_some())
        + usize::from(def.shl.is_some());
    if components != 1 {
        bail!(
            "constant {} must have exactly one of value, alias, or, shl",
            def.name
        );
    }
    if let Some(value) = &def.value {
        return match value {
            toml::Value::Integer(number) => Ok(i128::from(*number)),
            toml::Value::String(text) => parse_int_literal(text),
            other => bail!("constant {} has a non-integer value {other}", def.name),
        };
    }
    if let Some(alias) = &def.alias {
        return resolve(alias);
    }
    if let Some(terms) = &def.or {
        if terms.is_empty() {
            bail!("constant {} has an empty or list", def.name);
        }
        let mut value = 0i128;
        for term in terms {
            let term = match term {
                Term::Int(number) => i128::from(*number),
                Term::Name(name) => resolve(name)?,
            };
            value |= term;
        }
        return Ok(value);
    }
    let Some([left, right]) = def.shl else {
        bail!("constant {} has no value", def.name);
    };
    if !(0..=127).contains(&right) {
        bail!("constant {} has an out-of-range shift {right}", def.name);
    }
    let shifted = i128::from(left)
        .checked_shl(u32::try_from(right).unwrap_or(0))
        .ok_or_else(|| anyhow::anyhow!("constant {} overflows during shift", def.name))?;
    Ok(shifted)
}

/// Whether `value` fits the Rust integer type `ty`.
pub fn fits(value: i128, ty: &str) -> bool {
    match ty {
        "i8" => i128::from(i8::MIN) <= value && value <= i128::from(i8::MAX),
        "u8" => 0 <= value && value <= i128::from(u8::MAX),
        "i16" => i128::from(i16::MIN) <= value && value <= i128::from(i16::MAX),
        "u16" => 0 <= value && value <= i128::from(u16::MAX),
        "i32" => i128::from(i32::MIN) <= value && value <= i128::from(i32::MAX),
        "u32" => 0 <= value && value <= i128::from(u32::MAX),
        "i64" | "isize" => i128::from(i64::MIN) <= value && value <= i128::from(i64::MAX),
        "u64" | "usize" => 0 <= value && value <= i128::from(u64::MAX),
        _ => false,
    }
}

/// Render the value as a C token, with base, suffix and parentheses.
pub fn render_c(def: &ConstDef, value: i128, default_base: &str) -> String {
    let base = def.base.as_deref().unwrap_or(default_base);
    let suffix = def.suffix.as_deref().unwrap_or("");
    if let Some(alias) = &def.alias {
        return alias.clone();
    }
    if let Some(terms) = &def.or {
        let rendered: Vec<String> = terms
            .iter()
            .map(|term| match term {
                Term::Int(number) => format_integer(i128::from(*number), base, suffix),
                Term::Name(name) => name.clone(),
            })
            .collect();
        return format!("({})", rendered.join("|"));
    }
    if let Some([left, right]) = def.shl {
        return format!(
            "({} << {})",
            format_integer(i128::from(left), base, suffix),
            right
        );
    }
    let _ = value;
    format_integer(value, base, suffix)
}

/// Render the value as a Rust expression.
pub fn render_rust(def: &ConstDef, value: i128, default_base: &str) -> String {
    let base = def.base.as_deref().unwrap_or(default_base);
    if let Some(alias) = &def.alias {
        return alias.clone();
    }
    if let Some(terms) = &def.or {
        let rendered: Vec<String> = terms
            .iter()
            .map(|term| match term {
                Term::Int(number) => format_rust_integer(i128::from(*number), base),
                Term::Name(name) => name.clone(),
            })
            .collect();
        return rendered.join(" | ");
    }
    if let Some([left, right]) = def.shl {
        return format!("{} << {right}", format_rust_integer(i128::from(left), base));
    }
    format_rust_integer(value, base)
}

/// Render the value as an `i128` literal for the `ALL` table.
#[must_use]
pub fn render_i128(value: i128) -> String {
    format!("{value}i128")
}

fn format_integer(value: i128, base: &str, suffix: &str) -> String {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let digits = match base {
        "hex" => format!("0x{magnitude:x}"),
        "oct" => {
            if magnitude == 0 {
                "0".to_string()
            } else {
                format!("0{magnitude:o}")
            }
        }
        _ => magnitude.to_string(),
    };
    let literal = format!("{digits}{suffix}");
    if negative {
        format!("(-{literal})")
    } else {
        literal
    }
}

fn format_rust_integer(value: i128, base: &str) -> String {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let digits = match base {
        "hex" => format!("0x{magnitude:x}"),
        "oct" => {
            if magnitude == 0 {
                "0".to_string()
            } else {
                format!("0o{magnitude:o}")
            }
        }
        _ => magnitude.to_string(),
    };
    if negative {
        format!("-{digits}")
    } else {
        digits
    }
}

/// A merged view of the object-like defines of a group's musl sources.
#[derive(Debug, Default)]
pub struct DefineSet {
    /// name -> expansion text (last definition wins).
    pub definitions: BTreeMap<String, String>,
}

impl DefineSet {
    /// Read every object-like `#define` from the given files.
    pub fn from_files(files: &[std::path::PathBuf]) -> Result<Self> {
        let mut set = Self::default();
        for file in files {
            let text = std::fs::read_to_string(file)
                .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", file.display()))?;
            for (name, expansion) in parse_defines(&text) {
                set.definitions.insert(name, expansion);
            }
        }
        Ok(set)
    }

    /// Names in the set that start with one of `prefixes`.
    #[must_use]
    pub fn names_with_prefixes(&self, prefixes: &[String]) -> Vec<&str> {
        self.definitions
            .keys()
            .filter(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
            .map(String::as_str)
            .collect()
    }

    /// Whether the macro is an object-like constant expression, with its value.
    #[must_use]
    pub fn constant_value(&self, name: &str) -> Option<i128> {
        self.value_of(name, 0)
    }

    fn value_of(&self, name: &str, depth: usize) -> Option<i128> {
        if depth > 16 {
            return None;
        }
        let expansion = self.definitions.get(name)?;
        eval_expression(expansion, &mut |identifier| {
            self.value_of(identifier, depth + 1)
        })
    }
}

/// Parse object-like defines into name -> expansion.
#[must_use]
pub fn parse_defines(text: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        let Some(rest) = line.strip_prefix("#define") else {
            continue;
        };
        if !rest.starts_with([' ', '\t']) {
            continue;
        }
        let rest = rest.trim_start();
        let end = rest
            .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            .unwrap_or(rest.len());
        let (name, tail) = rest.split_at(end);
        if name.is_empty() {
            continue;
        }
        // Function-like: the opening parenthesis follows the name without whitespace.
        if tail.starts_with('(') {
            continue;
        }
        let expansion = tail.trim();
        if expansion.is_empty() {
            continue;
        }
        rows.push((name.to_string(), expansion.to_string()));
    }
    rows
}

/// Evaluate a C constant expression made of integers, identifiers, parentheses and `| & ^ << >> + -
/// * / ~`.
#[must_use]
pub fn eval_expression(
    text: &str,
    resolve_name: &mut dyn FnMut(&str) -> Option<i128>,
) -> Option<i128> {
    let tokens = tokenize(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        index: 0,
        resolve_name,
    };
    let value = parser.expression(0)?;
    (parser.index == tokens.len()).then_some(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(i128),
    Name(String),
    Op(char),
    ShiftLeft,
    ShiftRight,
}

fn tokenize(text: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character.is_whitespace() || character == '\\' {
            index += 1;
            continue;
        }
        if character.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric()
                    || chars[index] == 'x'
                    || chars[index] == 'X')
            {
                index += 1;
            }
            let literal: String = chars[start..index].iter().collect();
            tokens.push(Token::Number(parse_c_integer(&literal)?));
            continue;
        }
        if character.is_ascii_alphabetic() || character == '_' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            let name: String = chars[start..index].iter().collect();
            tokens.push(Token::Name(name));
            continue;
        }
        if character == '&' && chars.get(index + 1) == Some(&'&') {
            return None;
        }
        if character == '|' && chars.get(index + 1) == Some(&'|') {
            return None;
        }
        if character == '<' && chars.get(index + 1) == Some(&'<') {
            tokens.push(Token::ShiftLeft);
            index += 2;
            continue;
        }
        if character == '>' && chars.get(index + 1) == Some(&'>') {
            tokens.push(Token::ShiftRight);
            index += 2;
            continue;
        }
        if matches!(
            character,
            '(' | ')' | '|' | '&' | '^' | '+' | '-' | '*' | '/' | '~'
        ) {
            tokens.push(Token::Op(character));
            index += 1;
            continue;
        }
        return None;
    }
    Some(tokens)
}

fn parse_c_integer(literal: &str) -> Option<i128> {
    // C integer constants with a leading zero are octal. TOML forbids that spelling, so the
    // TOML reader never sees one; only the musl expression scan does.
    if literal.len() > 1
        && literal.starts_with('0')
        && !literal.starts_with("0x")
        && !literal.starts_with("0X")
    {
        let body = literal.trim_end_matches(['u', 'U', 'l', 'L']);
        if !body.is_empty()
            && body
                .chars()
                .all(|character| ('0'..='7').contains(&character))
        {
            return i128::from_str_radix(body, 8).ok();
        }
    }
    parse_int_literal(literal).ok()
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
    resolve_name: &'a mut dyn FnMut(&str) -> Option<i128>,
}

impl Parser<'_> {
    fn expression(&mut self, minimum: u8) -> Option<i128> {
        let mut left = self.unary()?;
        while let Some((precedence, operation)) = self.peek_binary() {
            if precedence < minimum {
                break;
            }
            self.index += 1;
            let right = self.expression(precedence + 1)?;
            left = apply(operation, left, right)?;
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<i128> {
        match self.tokens.get(self.index)? {
            Token::Op('~') => {
                self.index += 1;
                Some(!self.unary()?)
            }
            Token::Op('-') => {
                self.index += 1;
                Some(self.unary()?.checked_neg()?)
            }
            Token::Op('+') => {
                self.index += 1;
                self.unary()
            }
            Token::Op('(') => {
                // A single-identifier cast, `(in_addr_t) 0`: musl's INADDR_* constants use it.
                // Only when an expression follows the closing parenthesis; `(EAGAIN)` stays a
                // parenthesised expression.
                let cast = matches!(
                    (
                        self.tokens.get(self.index + 1),
                        self.tokens.get(self.index + 2),
                        self.tokens.get(self.index + 3),
                    ),
                    (
                        Some(Token::Name(_)),
                        Some(Token::Op(')')),
                        Some(
                            Token::Number(_)
                                | Token::Name(_)
                                | Token::Op('(')
                                | Token::Op('~')
                                | Token::Op('-')
                                | Token::Op('+')
                        )
                    )
                );
                if cast {
                    self.index += 3;
                    return self.unary();
                }
                self.index += 1;
                let value = self.expression(0)?;
                match self.tokens.get(self.index)? {
                    Token::Op(')') => {
                        self.index += 1;
                        Some(value)
                    }
                    _ => None,
                }
            }
            Token::Number(value) => {
                self.index += 1;
                Some(*value)
            }
            Token::Name(name) => {
                let value = (self.resolve_name)(name)?;
                self.index += 1;
                Some(value)
            }
            _ => None,
        }
    }

    fn peek_binary(&self) -> Option<(u8, char)> {
        match self.tokens.get(self.index)? {
            Token::Op('|') => Some((1, '|')),
            Token::Op('^') => Some((2, '^')),
            Token::Op('&') => Some((3, '&')),
            Token::ShiftLeft => Some((4, '<')),
            Token::ShiftRight => Some((4, '>')),
            Token::Op('+') => Some((5, '+')),
            Token::Op('-') => Some((5, '-')),
            Token::Op('*') => Some((6, '*')),
            Token::Op('/') => Some((6, '/')),
            _ => None,
        }
    }
}

fn apply(operation: char, left: i128, right: i128) -> Option<i128> {
    match operation {
        '|' => Some(left | right),
        '^' => Some(left ^ right),
        '&' => Some(left & right),
        '<' => left.checked_shl(u32::try_from(right).ok()?),
        '>' => left.checked_shr(u32::try_from(right).ok()?),
        '+' => left.checked_add(right),
        '-' => left.checked_sub(right),
        '*' => left.checked_mul(right),
        '/' => {
            if right == 0 {
                None
            } else {
                left.checked_div(right)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "values_ut.rs"]
mod tests;
