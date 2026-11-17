// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `lex` lexer generator.
//!
//! The tool parses the POSIX lex file structure (definitions, rules, user code), translates the
//! lex regular expressions to POSIX extended regular expressions, and emits a C scanner that uses
//! the host `regex.h` engine. The generated scanner implements rule order, longest match, start
//! conditions, `yytext`/`yyleng`, `BEGIN`, `ECHO` and `yywrap`.
//!
//! The bounded subset supports the operators `.`, `*`, `+`, `?`, `|`, `(...)`, `[...]`, `{m,n}`,
//! `^`, `$`, quoted strings, escapes, name definitions, `%s`/`%x` start conditions and actions.
//! Trailing context, `<<EOF>>`, `%option`, `REJECT`, `yyless`, `unput` and non-ASCII classes are
//! explicit unsupported cases.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![cfg(not(target_os = "none"))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt::Write as _;

/// One lex condition declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    /// Condition name.
    pub name: String,
    /// `%x` exclusive condition; unqualified rules are inactive in it.
    pub exclusive: bool,
}

/// One rule from the rules section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Lex regular expression, before definition expansion.
    pub pattern: String,
    /// Start conditions from the `<...>` prefix; empty for unqualified rules.
    pub conditions: Vec<String>,
    /// C action, including braces when the source used a block.
    pub action: String,
    /// Source line for diagnostics.
    pub line: usize,
}

/// A parsed lex specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    /// Code from `%{ ... %}` in the definitions section.
    pub prologue: String,
    /// Code after the second `%%`.
    pub epilogue: String,
    /// Start conditions, without `INITIAL`.
    pub conditions: Vec<Condition>,
    /// Name definitions in declaration order.
    pub definitions: Vec<(String, String)>,
    /// Rules in source order.
    pub rules: Vec<Rule>,
}

/// A translated regular expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// POSIX extended regular expression source.
    pub ere: String,
    /// Pattern began with `^`.
    pub begin: bool,
    /// Pattern ended with `$`.
    pub end: bool,
}

/// A lex parse, translation or generation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Source line, or 0 when the error has no line.
    pub line: usize,
    /// Human-readable text.
    pub message: String,
}

impl Error {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(formatter, "{}", self.message)
        } else {
            write!(formatter, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for Error {}

/// Parse a lex specification and emit the C scanner.
///
/// # Errors
/// Returns [`Error`] for a malformed specification, an unsupported construct or an empty rule set.
pub fn compile(source: &str) -> Result<String, Error> {
    let spec = Spec::parse(source)?;
    compile_spec(&spec)
}

impl Spec {
    /// Parse the three-section lex file format.
    ///
    /// # Errors
    /// Returns [`Error`] with a source line for malformed definitions, rules or actions.
    pub fn parse(source: &str) -> Result<Self, Error> {
        let lines: Vec<&str> = source.lines().collect();
        let first = lines
            .iter()
            .position(|line| line.trim() == "%%")
            .ok_or_else(|| Error::new(0, "missing first %% separator"))?;
        let second = lines
            .iter()
            .enumerate()
            .skip(first + 1)
            .find(|(_, line)| line.trim() == "%%")
            .map(|(index, _)| index);

        let mut spec = Self {
            prologue: String::new(),
            epilogue: String::new(),
            conditions: Vec::new(),
            definitions: Vec::new(),
            rules: Vec::new(),
        };
        parse_definitions(&lines[..first], &mut spec)?;
        let rules_end = second.unwrap_or(lines.len());
        parse_rules(&lines[first + 1..rules_end], first + 2, &mut spec)?;
        if let Some(index) = second {
            spec.epilogue = lines[index + 1..].join("\n");
        }
        resolve_shared_actions(&mut spec)?;
        if spec.rules.is_empty() {
            return Err(Error::new(0, "no rules"));
        }
        validate_conditions(&spec)?;
        Ok(spec)
    }
}

fn parse_definitions(lines: &[&str], spec: &mut Spec) -> Result<(), Error> {
    let mut index = 0usize;
    while index < lines.len() {
        let number = index + 1;
        let line = lines[index].trim();
        index += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("/*") {
            let mut closed = line.contains("*/");
            while !closed && index < lines.len() {
                closed = lines[index].contains("*/");
                index += 1;
            }
            if !closed {
                return Err(Error::new(number, "unterminated comment"));
            }
            continue;
        }
        if line == "%{" {
            let mut code = Vec::new();
            let mut closed = false;
            while index < lines.len() {
                let current = lines[index];
                index += 1;
                if current.trim() == "%}" {
                    closed = true;
                    break;
                }
                code.push(current);
            }
            if !closed {
                return Err(Error::new(number, "unterminated %{ code block"));
            }
            if !spec.prologue.is_empty() {
                spec.prologue.push('\n');
            }
            spec.prologue.push_str(&code.join("\n"));
            continue;
        }
        if line.starts_with("%s") || line.starts_with("%x") {
            let exclusive = line.starts_with("%x");
            let rest = line[2..].trim();
            if rest.is_empty() {
                return Err(Error::new(
                    number,
                    "start condition declaration needs a name",
                ));
            }
            for name in rest.split_whitespace() {
                if !is_name(name) {
                    return Err(Error::new(number, format!("invalid condition `{name}`")));
                }
                if name == "INITIAL" || spec.conditions.iter().any(|item| item.name == name) {
                    return Err(Error::new(number, format!("duplicate condition `{name}`")));
                }
                spec.conditions.push(Condition {
                    name: name.to_string(),
                    exclusive,
                });
            }
            continue;
        }
        if line.starts_with('%') {
            return Err(Error::new(
                number,
                format!(
                    "unsupported lex directive `{}`",
                    line.split_whitespace().next().unwrap_or(line)
                ),
            ));
        }
        let Some((name, pattern)) = split_definition(line) else {
            return Err(Error::new(number, "expected a name definition"));
        };
        if !is_name(name) {
            return Err(Error::new(
                number,
                format!("invalid definition name `{name}`"),
            ));
        }
        if spec.definitions.iter().any(|item| item.0 == name) {
            return Err(Error::new(number, format!("duplicate definition `{name}`")));
        }
        spec.definitions
            .push((name.to_string(), pattern.to_string()));
    }
    Ok(())
}

fn split_definition(line: &str) -> Option<(&str, &str)> {
    let index = line.find(char::is_whitespace)?;
    let name = &line[..index];
    let pattern = line[index..].trim();
    if pattern.is_empty() {
        None
    } else {
        Some((name, pattern))
    }
}

fn parse_rules(lines: &[&str], line_base: usize, spec: &mut Spec) -> Result<(), Error> {
    let mut index = 0usize;
    while index < lines.len() {
        let number = line_base + index;
        let raw = lines[index];
        let line = raw.trim();
        index += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("/*") {
            let mut closed = line.contains("*/");
            while !closed && index < lines.len() {
                closed = lines[index].contains("*/");
                index += 1;
            }
            if !closed {
                return Err(Error::new(number, "unterminated comment"));
            }
            continue;
        }
        if line.starts_with("%{") {
            return Err(Error::new(
                number,
                "code blocks in the rules section are unsupported",
            ));
        }
        let (conditions, rest) = split_conditions(line, number)?;
        if rest.starts_with("<<EOF>>") {
            return Err(Error::new(number, "the <<EOF>> rule is unsupported"));
        }
        let Some(end) = pattern_end(rest) else {
            return Err(Error::new(number, "rule has no action"));
        };
        let pattern = rest[..end].trim();
        if pattern.is_empty() {
            return Err(Error::new(number, "rule has no pattern"));
        }
        let action_text = rest[end..].trim();
        if action_text.is_empty() {
            return Err(Error::new(number, "rule has no action"));
        }
        let (action, consumed) = collect_action(action_text, lines, index, number)?;
        index = consumed;
        spec.rules.push(Rule {
            pattern: pattern.to_string(),
            conditions,
            action,
            line: number,
        });
    }
    Ok(())
}

fn split_conditions(line: &str, number: usize) -> Result<(Vec<String>, &str), Error> {
    if !line.starts_with('<') {
        return Ok((Vec::new(), line));
    }
    let Some(end) = line.find('>') else {
        return Err(Error::new(number, "unterminated start condition list"));
    };
    let text = &line[1..end];
    let rest = line[end + 1..].trim();
    if rest.is_empty() {
        return Err(Error::new(number, "start condition rule has no pattern"));
    }
    if text == "*" {
        return Ok((vec!["*".to_string()], rest));
    }
    let mut conditions = Vec::new();
    for name in text.split(',') {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::new(number, "empty start condition name"));
        }
        conditions.push(name.to_string());
    }
    Ok((conditions, rest))
}

/// Index of the first unquoted, unclassified whitespace in a rule.
fn pattern_end(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut class_depth = 0usize;
    let mut quote = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == b'"' {
                quote = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => quote = true,
            b'[' => class_depth += 1,
            b']' => class_depth = class_depth.saturating_sub(1),
            b'\\' => {
                index = index.saturating_add(2);
                continue;
            }
            b' ' | b'\t' if class_depth == 0 => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

fn collect_action(
    text: &str,
    lines: &[&str],
    mut index: usize,
    number: usize,
) -> Result<(String, usize), Error> {
    if !text.starts_with('{') {
        return Ok((text.to_string(), index));
    }
    let mut action = String::new();
    action.push_str(text);
    let mut balance = brace_balance(text);
    while balance > 0 {
        if index >= lines.len() {
            return Err(Error::new(number, "unterminated action block"));
        }
        let next = lines[index];
        index += 1;
        action.push('\n');
        action.push_str(next);
        balance += brace_balance(next);
    }
    Ok((action, index))
}

fn brace_balance(text: &str) -> i32 {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut balance = 0i32;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(current) = quote {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == current {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() {
                if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            break;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'{' => balance += 1,
            b'}' => balance -= 1,
            _ => {}
        }
        index += 1;
    }
    balance
}

fn resolve_shared_actions(spec: &mut Spec) -> Result<(), Error> {
    for index in (0..spec.rules.len()).rev() {
        if spec.rules[index].action == "|" {
            let next = spec.rules[index + 1..]
                .iter()
                .find(|rule| rule.action != "|")
                .map(|rule| rule.action.clone());
            let Some(action) = next else {
                return Err(Error::new(
                    spec.rules[index].line,
                    "the last rule cannot share the next action",
                ));
            };
            spec.rules[index].action = action;
        }
    }
    Ok(())
}

fn validate_conditions(spec: &Spec) -> Result<(), Error> {
    for rule in &spec.rules {
        for name in &rule.conditions {
            if name == "*" {
                continue;
            }
            if name == "INITIAL" {
                continue;
            }
            if !spec
                .conditions
                .iter()
                .any(|condition| &condition.name == name)
            {
                return Err(Error::new(
                    rule.line,
                    format!("unknown start condition `{name}`"),
                ));
            }
        }
    }
    Ok(())
}

/// Expand `{NAME}` name definitions in a lex pattern.
///
/// # Errors
/// Returns [`Error`] for an undefined name or excessive nesting.
pub fn expand_definitions(
    pattern: &str,
    definitions: &[(String, String)],
) -> Result<String, Error> {
    expand_definitions_at(pattern, definitions, 0)
}

fn expand_definitions_at(
    pattern: &str,
    definitions: &[(String, String)],
    depth: usize,
) -> Result<String, Error> {
    if depth > 16 {
        return Err(Error::new(0, "definition expansion is too deep"));
    }
    let bytes = pattern.as_bytes();
    let mut output = String::new();
    let mut index = 0usize;
    let mut class_depth = 0usize;
    let mut quote = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if quote {
            output.push(byte as char);
            if byte == b'\\'
                && let Some(next) = bytes.get(index + 1)
            {
                output.push(*next as char);
                index += 2;
                continue;
            }
            if byte == b'"' {
                quote = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                quote = true;
                output.push('"');
                index += 1;
            }
            b'[' => {
                class_depth += 1;
                output.push('[');
                index += 1;
            }
            b']' => {
                class_depth = class_depth.saturating_sub(1);
                output.push(']');
                index += 1;
            }
            b'\\' => {
                output.push('\\');
                if let Some(next) = bytes.get(index + 1) {
                    output.push(*next as char);
                    index += 2;
                } else {
                    index += 1;
                }
            }
            b'{' if class_depth == 0 => {
                let Some(end) = pattern[index..].find('}') else {
                    return Err(Error::new(0, "unterminated definition reference"));
                };
                let end = index + end;
                let name = &pattern[index + 1..end];
                if is_repetition(name) {
                    output.push_str(&pattern[index..=end]);
                    index = end + 1;
                    continue;
                }
                if !is_name(name) {
                    return Err(Error::new(
                        0,
                        format!("invalid definition reference `{{{name}}}`"),
                    ));
                }
                let Some((_, value)) = definitions.iter().find(|item| item.0 == name) else {
                    return Err(Error::new(0, format!("undefined definition `{name}`")));
                };
                output.push('(');
                output.push_str(&expand_definitions_at(value, definitions, depth + 1)?);
                output.push(')');
                index = end + 1;
            }
            _ => {
                output.push(byte as char);
                index += 1;
            }
        }
    }
    Ok(output)
}

fn is_repetition(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b',')
        && text.bytes().any(|byte| byte.is_ascii_digit())
}

fn is_name(text: &str) -> bool {
    let mut bytes = text.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Translate one lex pattern to a POSIX extended regular expression and anchors.
///
/// # Errors
/// Returns [`Error`] for an unsupported construct or an unbalanced class/quote.
pub fn translate_pattern(
    pattern: &str,
    definitions: &[(String, String)],
) -> Result<Pattern, Error> {
    let expanded = expand_definitions(pattern, definitions)?;
    let mut body = expanded.trim();
    if body.is_empty() {
        return Err(Error::new(0, "empty pattern"));
    }
    let begin = body.starts_with('^');
    if begin {
        body = &body[1..];
    }
    let end = ends_with_unescaped_dollar(body);
    if end {
        body = &body[..body.len() - 1];
    }
    if body.is_empty() {
        return Err(Error::new(0, "empty pattern after anchors"));
    }
    let ere = translate_body(body)?;
    if ere.is_empty() {
        return Err(Error::new(0, "pattern translates to an empty expression"));
    }
    Ok(Pattern { ere, begin, end })
}

fn ends_with_unescaped_dollar(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some(last) = bytes.last() else {
        return false;
    };
    if *last != b'$' {
        return false;
    }
    let mut backslashes = 0usize;
    let mut index = bytes.len() - 1;
    while index > 0 {
        index -= 1;
        if bytes[index] == b'\\' {
            backslashes += 1;
        } else {
            break;
        }
    }
    backslashes.is_multiple_of(2)
}

fn translate_body(text: &str) -> Result<String, Error> {
    let bytes = text.as_bytes();
    let mut output = String::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\\' => {
                let Some(next) = bytes.get(index + 1) else {
                    return Err(Error::new(0, "trailing backslash"));
                };
                push_literal(&mut output, escape_byte(*next));
                index += 2;
            }
            b'"' => {
                let (literal, next) = read_quoted(bytes, index)?;
                for byte in literal {
                    push_literal(&mut output, byte);
                }
                index = next;
            }
            b'[' => {
                let (class, next) = read_class(bytes, index)?;
                output.push_str(&class);
                index = next;
            }
            b'.' => {
                output.push('.');
                index += 1;
            }
            b'*' | b'+' | b'?' | b'|' | b'(' | b')' => {
                output.push(byte as char);
                index += 1;
            }
            b'{' => {
                let Some(end) = text[index..].find('}') else {
                    return Err(Error::new(0, "unterminated repetition"));
                };
                let end = index + end;
                let content = &text[index + 1..end];
                if !is_repetition(content) {
                    return Err(Error::new(0, format!("invalid repetition `{{{content}}}`")));
                }
                output.push_str(&text[index..=end]);
                index = end + 1;
            }
            b'/' => {
                return Err(Error::new(0, "trailing context is unsupported"));
            }
            b'^' | b'$' => {
                output.push('\\');
                output.push(byte as char);
                index += 1;
            }
            _ => {
                push_literal(&mut output, byte);
                index += 1;
            }
        }
    }
    Ok(output)
}

fn read_quoted(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize), Error> {
    let mut literal = Vec::new();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return Ok((literal, index + 1)),
            b'\\' => {
                let Some(next) = bytes.get(index + 1) else {
                    return Err(Error::new(0, "trailing backslash in a quoted string"));
                };
                literal.push(escape_byte(*next));
                index += 2;
            }
            byte => {
                literal.push(byte);
                index += 1;
            }
        }
    }
    Err(Error::new(0, "unterminated quoted string"))
}

fn read_class(bytes: &[u8], start: usize) -> Result<(String, usize), Error> {
    let mut class = String::from("[");
    let mut index = start + 1;
    if bytes.get(index) == Some(&b'^') {
        class.push('^');
        index += 1;
    }
    let mut first = true;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b']' && !first {
            class.push(']');
            return Ok((class, index + 1));
        }
        first = false;
        if byte == b'[' && bytes.get(index + 1) == Some(&b':') {
            let Some(end) = find_class_end(bytes, index) else {
                return Err(Error::new(0, "unterminated POSIX character class"));
            };
            class.push_str(std::str::from_utf8(&bytes[index..end]).unwrap_or(""));
            index = end;
            continue;
        }
        if byte == b'\\' {
            let Some(next) = bytes.get(index + 1) else {
                return Err(Error::new(0, "trailing backslash in a character class"));
            };
            let escaped = escape_byte(*next);
            if matches!(escaped, b']' | b'-' | b'^' | b'\\') {
                class.push('\\');
            }
            class.push(escaped as char);
            index += 2;
            continue;
        }
        class.push(byte as char);
        index += 1;
    }
    Err(Error::new(0, "unterminated character class"))
}

fn find_class_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b':' && bytes[index + 1] == b']' {
            return Some(index + 2);
        }
        index += 1;
    }
    None
}

fn escape_byte(byte: u8) -> u8 {
    match byte {
        b'n' => b'\n',
        b't' => b'\t',
        b'r' => b'\r',
        b'f' => 0x0c,
        b'v' => 0x0b,
        b'a' => 0x07,
        b'b' => 0x08,
        other => other,
    }
}

fn push_literal(output: &mut String, byte: u8) {
    if matches!(
        byte,
        b'.' | b'^'
            | b'$'
            | b'['
            | b']'
            | b'('
            | b')'
            | b'|'
            | b'*'
            | b'+'
            | b'?'
            | b'{'
            | b'}'
            | b'\\'
    ) {
        output.push('\\');
    }
    output.push(byte as char);
}

/// Emit the C scanner for a parsed specification.
///
/// # Errors
/// Returns [`Error`] for too many conditions, an unknown condition or a bad action mapping.
pub fn compile_spec(spec: &Spec) -> Result<String, Error> {
    let mut conditions = vec![Condition {
        name: "INITIAL".to_string(),
        exclusive: false,
    }];
    conditions.extend(spec.conditions.iter().cloned());
    if conditions.len() > 63 {
        return Err(Error::new(0, "at most 63 start conditions are supported"));
    }
    let mut compiled = Vec::new();
    for rule in &spec.rules {
        let pattern = translate_pattern(&rule.pattern, &spec.definitions)
            .map_err(|error| Error::new(rule.line, error.message))?;
        let mask = condition_mask(rule, &conditions)?;
        compiled.push((pattern, mask, rule.action.clone()));
    }
    let mut output = String::new();
    output.push_str("/* Generated by RavenBSD lex. */\n");
    output.push_str(
        "#include <regex.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\n",
    );
    if !spec.prologue.is_empty() {
        output.push_str(&spec.prologue);
        output.push('\n');
    }
    for (index, condition) in conditions.iter().enumerate() {
        if index == 0 {
            output.push_str("#define INITIAL 0\n");
        } else {
            let _ = writeln!(output, "#define {} {}", condition.name, index);
        }
    }
    output.push_str("#define BEGIN(s) (yy_start = (s))\n");
    output.push_str("#define ECHO fwrite(yytext, 1, (size_t)yyleng, yyout)\n\n");
    output.push_str("FILE *yyin = NULL;\nFILE *yyout = NULL;\nchar *yytext = NULL;\nint yyleng = 0;\nint yy_start = 0;\n\n");
    let _ = writeln!(output, "#define YY_RULE_COUNT {}", compiled.len());
    output.push_str("static const char *yy_pattern[YY_RULE_COUNT] = {\n");
    for (pattern, _, _) in &compiled {
        let _ = writeln!(output, "    {},", c_string(&pattern.ere));
    }
    output.push_str("};\n");
    output.push_str("static const unsigned long long yy_cond[YY_RULE_COUNT] = {\n");
    for (_, mask, _) in &compiled {
        let _ = writeln!(output, "    0x{mask:016x}ULL,");
    }
    output.push_str("};\n");
    output.push_str("static const unsigned char yy_begin[YY_RULE_COUNT] = {\n");
    for (pattern, _, _) in &compiled {
        let _ = writeln!(output, "    {},", u8::from(pattern.begin));
    }
    output.push_str("};\n");
    output.push_str("static const unsigned char yy_end[YY_RULE_COUNT] = {\n");
    for (pattern, _, _) in &compiled {
        let _ = writeln!(output, "    {},", u8::from(pattern.end));
    }
    output.push_str("};\n");
    output.push_str("static regex_t yy_regex[YY_RULE_COUNT];\nstatic int yy_ready = 0;\n\n");
    output.push_str(
        "static int yy_compile(void) {\n\
         \x20   int index;\n\
         \x20   char message[256];\n\
         \x20   for (index = 0; index < YY_RULE_COUNT; index++) {\n\
         \x20       int status = regcomp(&yy_regex[index], yy_pattern[index], REG_EXTENDED | REG_NEWLINE);\n\
         \x20       if (status != 0) {\n\
         \x20           regerror(status, &yy_regex[index], message, sizeof(message));\n\
         \x20           fprintf(stderr, \"lex: rule %d: %s\\n\", index + 1, message);\n\
         \x20           return 0;\n\
         \x20       }\n\
         \x20   }\n\
         \x20   return 1;\n\
         }\n\n",
    );
    output.push_str(
        "static char *yy_buf = NULL;\n\
         static size_t yy_len = 0;\n\
         static size_t yy_pos = 0;\n\
         static int yy_loaded = 0;\n\
         static char *yy_text = NULL;\n\
         static size_t yy_text_cap = 0;\n\n\
         static int yy_load(void) {\n\
         \x20   size_t capacity = 4096;\n\
         \x20   size_t length = 0;\n\
         \x20   char *buffer = (char *)malloc(capacity + 1);\n\
         \x20   if (buffer == NULL) return 0;\n\
         \x20   for (;;) {\n\
         \x20       if (length == capacity) {\n\
         \x20           size_t next = capacity * 2;\n\
         \x20           char *grown = (char *)realloc(buffer, next + 1);\n\
         \x20           if (grown == NULL) { free(buffer); return 0; }\n\
         \x20           buffer = grown;\n\
         \x20           capacity = next;\n\
         \x20       }\n\
         \x20       size_t count = fread(buffer + length, 1, capacity - length, yyin);\n\
         \x20       length += count;\n\
         \x20       if (count == 0) break;\n\
         \x20   }\n\
         \x20   buffer[length] = '\\0';\n\
         \x20   yy_buf = buffer;\n\
         \x20   yy_len = length;\n\
         \x20   yy_pos = 0;\n\
         \x20   yy_loaded = 1;\n\
         \x20   return 1;\n\
         }\n\n\
         static void yy_set_text(size_t start, size_t length) {\n\
         \x20   if (length + 1 > yy_text_cap) {\n\
         \x20       size_t next = length + 64;\n\
         \x20       char *grown = (char *)realloc(yy_text, next);\n\
         \x20       if (grown == NULL) { fprintf(stderr, \"lex: out of memory\\n\"); exit(1); }\n\
         \x20       yy_text = grown;\n\
         \x20       yy_text_cap = next;\n\
         \x20   }\n\
         \x20   memcpy(yy_text, yy_buf + start, length);\n\
         \x20   yy_text[length] = '\\0';\n\
         \x20   yytext = yy_text;\n\
         \x20   yyleng = (int)length;\n\
         }\n\n",
    );
    if !spec.prologue.contains("yywrap") && !spec.epilogue.contains("yywrap") {
        output.push_str("int yywrap(void) { return 1; }\n\n");
    }
    output.push_str(
        "int yylex(void) {\n\
         \x20   if (yyin == NULL) yyin = stdin;\n\
         \x20   if (yyout == NULL) yyout = stdout;\n\
         \x20   if (!yy_ready) { if (!yy_compile()) return 0; yy_ready = 1; }\n\
         \x20   if (!yy_loaded && !yy_load()) return 0;\n\
         \x20   for (;;) {\n\
         \x20       if (yy_pos >= yy_len) {\n\
         \x20           if (yywrap()) return 0;\n\
         \x20           yy_loaded = 0;\n\
         \x20           if (!yy_load()) return 0;\n\
         \x20           continue;\n\
         \x20       }\n\
         \x20       int at_bol = (yy_pos == 0 || yy_buf[yy_pos - 1] == '\\n');\n\
         \x20       int best = -1;\n\
         \x20       size_t best_len = 0;\n\
         \x20       int rule;\n\
         \x20       for (rule = 0; rule < YY_RULE_COUNT; rule++) {\n\
         \x20           if ((yy_cond[rule] & (1ULL << yy_start)) == 0) continue;\n\
         \x20           if (yy_begin[rule] && !at_bol) continue;\n\
         \x20           regmatch_t match;\n\
         \x20           int flags = at_bol ? 0 : REG_NOTBOL;\n\
         \x20           if (regexec(&yy_regex[rule], yy_buf + yy_pos, 1, &match, flags) != 0) continue;\n\
         \x20           if (match.rm_so != 0 || match.rm_eo <= 0) continue;\n\
         \x20           size_t length = (size_t)match.rm_eo;\n\
         \x20           if (yy_end[rule] && !(yy_pos + length == yy_len || yy_buf[yy_pos + length] == '\\n')) continue;\n\
         \x20           if (length > best_len) { best = rule; best_len = length; }\n\
         \x20       }\n\
         \x20       if (best >= 0) {\n\
         \x20           yy_set_text(yy_pos, best_len);\n\
         \x20           yy_pos += best_len;\n\
         \x20           switch (best) {\n",
    );
    for (index, (_, _, action)) in compiled.iter().enumerate() {
        let _ = writeln!(output, "            case {index}:");
        output.push_str("                ");
        output.push_str(action);
        output.push_str("\n                break;\n");
    }
    output.push_str(
        "           }\n\
         \x20           continue;\n\
         \x20       }\n\
         \x20       yy_set_text(yy_pos, 1);\n\
         \x20       yy_pos++;\n\
         \x20       ECHO;\n\
         \x20   }\n\
         }\n\n",
    );
    if !spec.epilogue.is_empty() {
        output.push_str(&spec.epilogue);
        output.push('\n');
    }
    Ok(output)
}

fn condition_mask(rule: &Rule, conditions: &[Condition]) -> Result<u64, Error> {
    if rule.conditions.is_empty() {
        let mut mask = 1u64;
        for (index, condition) in conditions.iter().enumerate().skip(1) {
            if !condition.exclusive {
                mask |= 1u64 << index;
            }
        }
        return Ok(mask);
    }
    if rule.conditions.len() == 1 && rule.conditions[0] == "*" {
        return Ok(u64::MAX);
    }
    let mut mask = 0u64;
    for name in &rule.conditions {
        let Some(index) = conditions
            .iter()
            .position(|condition| &condition.name == name)
        else {
            return Err(Error::new(
                rule.line,
                format!("unknown start condition `{name}`"),
            ));
        };
        mask |= 1u64 << index;
    }
    Ok(mask)
}

fn c_string(text: &str) -> String {
    let mut output = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'\\' => output.push_str("\\\\"),
            b'"' => output.push_str("\\\""),
            b'\n' => output.push_str("\\n"),
            b'\r' => output.push_str("\\r"),
            b'\t' => output.push_str("\\t"),
            0x20..=0x7e => output.push(byte as char),
            _ => {
                let _ = write!(output, "\\x{byte:02x}");
            }
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
