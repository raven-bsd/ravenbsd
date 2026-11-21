// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C function-definition extraction for the pinned `rack.c`.
//!
//! The parser strips comments, string/character literals and preprocessor directives, then
//! walks brace depth zero. A definition is an identifier followed by a parameter list whose
//! closing parenthesis is followed by `{`; prototypes (`;`) and macro calls inside bodies are
//! ignored. This is a structural parser, not a C compiler: it is deliberately limited to the
//! FreeBSD kernel style used by `sys/netinet/tcp_stacks/rack.c`.

/// One function definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Function {
    /// Source line of the identifier (1-based).
    pub line: usize,
    /// Function name.
    pub name: String,
}

/// Parse a donor C file's top-level function definitions.
#[must_use]
pub fn parse_functions(source: &str) -> Vec<Function> {
    let stripped = strip_preprocessor(&strip_comments_and_literals(source));
    let bytes = stripped.as_bytes();
    let mut line_starts = vec![0usize];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            line_starts.push(index + 1);
        }
    }
    let line_of = |position: usize| {
        line_starts
            .partition_point(|start| *start <= position)
            .max(1)
    };

    let mut functions = Vec::new();
    let mut index = 0usize;
    let mut depth = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'{' => {
                depth += 1;
                index += 1;
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            byte if depth == 0 && is_ident_start(byte) => {
                let start = index;
                index += 1;
                while index < bytes.len() && is_ident_continue(bytes[index]) {
                    index += 1;
                }
                let name = &stripped[start..index];
                if is_keyword(name) {
                    continue;
                }
                let mut cursor = index;
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor >= bytes.len() || bytes[cursor] != b'(' {
                    continue;
                }
                cursor += 1;
                let mut parens = 1usize;
                while cursor < bytes.len() && parens > 0 {
                    match bytes[cursor] {
                        b'(' => parens += 1,
                        b')' => parens -= 1,
                        _ => {}
                    }
                    cursor += 1;
                }
                if parens != 0 {
                    continue;
                }
                // Skip attributes/asm and whitespace up to the body or a declaration end.
                let mut body = cursor;
                let mut found_body = false;
                while body < bytes.len() {
                    match bytes[body] {
                        b'{' => {
                            found_body = true;
                            break;
                        }
                        b';' => break,
                        b'(' => {
                            let mut nested = 1usize;
                            body += 1;
                            while body < bytes.len() && nested > 0 {
                                match bytes[body] {
                                    b'(' => nested += 1,
                                    b')' => nested -= 1,
                                    _ => {}
                                }
                                body += 1;
                            }
                        }
                        _ => body += 1,
                    }
                }
                if !found_body {
                    continue;
                }
                functions.push(Function {
                    line: line_of(start),
                    name: name.to_owned(),
                });
                let mut braces = 0usize;
                while body < bytes.len() {
                    match bytes[body] {
                        b'{' => braces += 1,
                        b'}' => {
                            braces = braces.saturating_sub(1);
                            if braces == 0 {
                                body += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    body += 1;
                }
                index = body;
            }
            _ => index += 1,
        }
    }
    functions
}

/// Parse the checked-in `line name` manifest.
#[must_use]
pub fn parse_manifest(text: &str) -> Vec<Function> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut parts = line.split_whitespace();
            let number = parts.next()?.parse::<usize>().ok()?;
            let name = parts.next()?.to_owned();
            Some(Function { line: number, name })
        })
        .collect()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "for"
            | "while"
            | "switch"
            | "sizeof"
            | "return"
            | "typeof"
            | "__typeof__"
            | "__attribute__"
            | "do"
            | "else"
            | "case"
            | "goto"
    )
}

/// Remove comments and string/character literal contents, preserving newlines.
fn strip_comments_and_literals(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        if byte == b'/' && next == Some(b'*') {
            out.push(' ');
            out.push(' ');
            index += 2;
            while index < bytes.len() {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    break;
                }
                out.push(if bytes[index] == b'\n' { '\n' } else { ' ' });
                index += 1;
            }
        } else if byte == b'/' && next == Some(b'/') {
            out.push(' ');
            out.push(' ');
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                out.push(' ');
                index += 1;
            }
        } else if byte == b'"' || byte == b'\'' {
            let quote = byte;
            out.push(' ');
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    out.push(' ');
                    index += 1;
                    if index < bytes.len() {
                        out.push(if bytes[index] == b'\n' { '\n' } else { ' ' });
                        index += 1;
                    }
                } else if bytes[index] == quote {
                    out.push(' ');
                    index += 1;
                    break;
                } else {
                    out.push(if bytes[index] == b'\n' { '\n' } else { ' ' });
                    index += 1;
                }
            }
        } else {
            out.push(char::from(byte));
            index += 1;
        }
    }
    out
}

/// Blank preprocessor directives, including backslash continuations.
fn strip_preprocessor(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut continuation = false;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let directive = continuation || trimmed.starts_with('#');
        continuation = directive && line.trim_end_matches(['\r', '\n']).ends_with('\\');
        if directive {
            out.extend(
                line.chars()
                    .map(|character| if character == '\n' { '\n' } else { ' ' }),
            );
        } else {
            out.push_str(line);
        }
    }
    out
}
