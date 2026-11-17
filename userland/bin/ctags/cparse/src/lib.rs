// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Small C tokenizer and analyzer shared by `cflow`, `ctags` and `cxref`.
//!
//! The analyzer recognizes macro definitions, top-level globals, function definitions,
//! `struct`/`enum` tags and identifier references. It is a lexical tool, not a C compiler:
//! expressions, types and preprocessor conditionals are not evaluated.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// What one symbol is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    /// Function definition.
    Function,
    /// Top-level object.
    Global,
    /// `#define` macro.
    Macro,
    /// `struct` tag.
    Struct,
    /// `enum` tag.
    Enum,
}

impl SymbolKind {
    /// Stable spelling used by the tools.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Global => "global",
            Self::Macro => "macro",
            Self::Struct => "struct",
            Self::Enum => "enum",
        }
    }
}

/// One definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// Identifier.
    pub name: String,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Source line.
    pub line: u32,
}

/// One function definition and its direct calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// Definition line.
    pub line: u32,
    /// Known function names called from the body, first-use order.
    pub calls: Vec<String>,
}

/// One identifier occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// Identifier.
    pub name: String,
    /// Source line.
    pub line: u32,
}

/// The result of analyzing one source file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    /// Definitions in source order.
    pub symbols: Vec<Symbol>,
    /// Function definitions in source order.
    pub functions: Vec<Function>,
    /// Every identifier occurrence in source order.
    pub references: Vec<Reference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Ident,
    Number,
    Punct(char),
    Directive,
    Literal,
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    line: u32,
    kind: TokenKind,
}

impl Token {
    fn is_identifier(&self) -> bool {
        self.kind == TokenKind::Ident
    }

    fn is_ident(&self, value: &str) -> bool {
        self.kind == TokenKind::Ident && self.text == value
    }

    fn is_punct(&self, value: char) -> bool {
        matches!(self.kind, TokenKind::Punct(character) if character == value)
    }
}

fn is_ident_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_ident_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn is_keyword(value: &str) -> bool {
    matches!(
        value,
        "auto"
            | "break"
            | "case"
            | "char"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extern"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "register"
            | "restrict"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "typedef"
            | "union"
            | "unsigned"
            | "void"
            | "volatile"
            | "while"
            | "_Alignas"
            | "_Alignof"
            | "_Atomic"
            | "_Bool"
            | "_Complex"
            | "_Generic"
            | "_Imaginary"
            | "_Noreturn"
            | "_Static_assert"
            | "_Thread_local"
    )
}

fn tokenize(source: &str) -> Vec<Token> {
    let characters = source.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1u32;
    let mut at_line_start = true;
    while index < characters.len() {
        let character = characters[index];
        if character == '\n' {
            line += 1;
            index += 1;
            at_line_start = true;
            continue;
        }
        if matches!(character, ' ' | '\t' | '\r' | '\x0c') {
            index += 1;
            continue;
        }
        if character == '/' && index + 1 < characters.len() && characters[index + 1] == '/' {
            index += 2;
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if character == '/' && index + 1 < characters.len() && characters[index + 1] == '*' {
            index += 2;
            let mut saw_newline = false;
            while index + 1 < characters.len()
                && !(characters[index] == '*' && characters[index + 1] == '/')
            {
                if characters[index] == '\n' {
                    line += 1;
                    saw_newline = true;
                }
                index += 1;
            }
            index = (index + 2).min(characters.len());
            if saw_newline {
                at_line_start = true;
            }
            continue;
        }
        if character == '#' && at_line_start {
            let start_line = line;
            let mut text = String::new();
            loop {
                let start = index;
                while index < characters.len() && characters[index] != '\n' {
                    index += 1;
                }
                let part = characters[start..index].iter().collect::<String>();
                let trimmed = part.trim_end();
                let continued = trimmed.ends_with('\\');
                let clean = if continued {
                    &trimmed[..trimmed.len() - 1]
                } else {
                    trimmed
                };
                text.push_str(clean);
                if index < characters.len() {
                    index += 1;
                    line += 1;
                }
                if continued {
                    text.push(' ');
                    continue;
                }
                break;
            }
            tokens.push(Token {
                text: text.trim_end().to_string(),
                line: start_line,
                kind: TokenKind::Directive,
            });
            at_line_start = true;
            continue;
        }
        if is_ident_start(character) {
            let start = index;
            index += 1;
            while index < characters.len() && is_ident_continue(characters[index]) {
                index += 1;
            }
            tokens.push(Token {
                text: characters[start..index].iter().collect(),
                line,
                kind: TokenKind::Ident,
            });
            at_line_start = false;
            continue;
        }
        if character.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < characters.len()
                && (is_ident_continue(characters[index]) || characters[index] == '.')
            {
                index += 1;
            }
            tokens.push(Token {
                text: characters[start..index].iter().collect(),
                line,
                kind: TokenKind::Number,
            });
            at_line_start = false;
            continue;
        }
        if matches!(character, '"' | '\'') {
            let quote = character;
            index += 1;
            while index < characters.len() {
                if characters[index] == '\\' {
                    index = (index + 2).min(characters.len());
                    continue;
                }
                if characters[index] == '\n' {
                    line += 1;
                }
                if characters[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            tokens.push(Token {
                text: quote.to_string(),
                line,
                kind: TokenKind::Literal,
            });
            at_line_start = false;
            continue;
        }
        tokens.push(Token {
            text: character.to_string(),
            line,
            kind: TokenKind::Punct(character),
        });
        index += 1;
        at_line_start = false;
    }
    tokens
}

fn parse_define(token: &Token, symbols: &mut Vec<Symbol>) {
    let text = token.text.trim_start_matches('#').trim();
    let mut parts = text.split_whitespace();
    if parts.next() != Some("define") {
        return;
    }
    let Some(raw) = parts.next() else {
        return;
    };
    let name = raw.split('(').next().unwrap_or(raw);
    if name.is_empty() || !name.chars().next().is_some_and(is_ident_start) {
        return;
    }
    if !name.chars().all(is_ident_continue) {
        return;
    }
    symbols.push(Symbol {
        name: name.to_string(),
        kind: SymbolKind::Macro,
        line: token.line,
    });
}

fn matching_brace(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token.is_punct('{') {
            depth += 1;
        } else if token.is_punct('}') {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Terminator {
    Semicolon,
    Brace,
    Close,
    Eof,
}

fn scan_declaration(tokens: &[Token], start: usize) -> (usize, Terminator) {
    let mut paren = 0i32;
    let mut bracket = 0i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.is_punct('(') {
            paren += 1;
            continue;
        }
        if token.is_punct(')') {
            paren -= 1;
            continue;
        }
        if token.is_punct('[') {
            bracket += 1;
            continue;
        }
        if token.is_punct(']') {
            bracket -= 1;
            continue;
        }
        if paren != 0 || bracket != 0 {
            continue;
        }
        if token.is_punct(';') {
            return (index, Terminator::Semicolon);
        }
        if token.is_punct('{') {
            return (index, Terminator::Brace);
        }
        if token.is_punct('}') {
            return (index, Terminator::Close);
        }
    }
    (tokens.len(), Terminator::Eof)
}

fn declarator_name(segment: &[Token]) -> Option<String> {
    let end = segment
        .iter()
        .position(|token| token.is_punct('='))
        .unwrap_or(segment.len());
    segment[..end]
        .iter()
        .rev()
        .find(|token| token.is_identifier() && !is_keyword(&token.text))
        .map(|token| token.text.clone())
}

fn record_declarators(tokens: &[Token], start: usize, end: usize, symbols: &mut Vec<Symbol>) {
    if end <= start {
        return;
    }
    let header = &tokens[start..end];
    if header[0].is_ident("typedef") || header[0].is_ident("extern") {
        return;
    }
    if header.len() == 2
        && (header[0].is_ident("struct") || header[0].is_ident("enum"))
        && header[1].is_identifier()
    {
        return;
    }
    if header.iter().any(|token| token.is_punct('(')) {
        return;
    }
    let mut segment_start = 0;
    for index in 0..=header.len() {
        let at_comma = index < header.len() && header[index].is_punct(',');
        if index == header.len() || at_comma {
            if let Some(name) = declarator_name(&header[segment_start..index]) {
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Global,
                    line: header[segment_start].line,
                });
            }
            segment_start = index + 1;
        }
    }
}

fn scan_typedef_end(tokens: &[Token], start: usize) -> usize {
    let mut brace = 0i32;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        if token.is_punct('{') {
            brace += 1;
        } else if token.is_punct('}') {
            brace -= 1;
        } else if token.is_punct(';') && brace == 0 {
            return index + 1;
        }
    }
    tokens.len()
}

fn function_header(tokens: &[Token], start: usize, brace: usize) -> Option<(String, u32)> {
    let header = &tokens[start..brace];
    let open = header.iter().position(|token| token.is_punct('('))?;
    if open == 0 {
        return None;
    }
    let name = &header[open - 1];
    if !name.is_identifier() || is_keyword(&name.text) {
        return None;
    }
    let mut depth = 0i32;
    let mut close = None;
    for (index, token) in header.iter().enumerate().skip(open) {
        if token.is_punct('(') {
            depth += 1;
        } else if token.is_punct(')') {
            depth -= 1;
            if depth == 0 {
                close = Some(index);
                break;
            }
        }
    }
    let close = close?;
    if header[close + 1..]
        .iter()
        .any(|token| token.is_punct('=') || token.is_punct(';'))
    {
        return None;
    }
    Some((name.text.clone(), name.line))
}

fn parse_type(tokens: &[Token], start: usize, symbols: &mut Vec<Symbol>) -> Option<usize> {
    let kind = if tokens[start].is_ident("struct") {
        SymbolKind::Struct
    } else if tokens[start].is_ident("enum") {
        SymbolKind::Enum
    } else {
        return None;
    };
    let name = tokens.get(start + 1)?;
    if !name.is_identifier() || is_keyword(&name.text) {
        return None;
    }
    if !tokens
        .get(start + 2)
        .is_some_and(|token| token.is_punct('{'))
    {
        return None;
    }
    symbols.push(Symbol {
        name: name.text.clone(),
        kind,
        line: name.line,
    });
    let close = matching_brace(tokens, start + 2)?;
    let mut index = close + 1;
    if index < tokens.len() && tokens[index].is_punct(';') {
        return Some(index + 1);
    }
    let (end, terminator) = scan_declaration(tokens, index);
    if terminator == Terminator::Semicolon {
        record_declarators(tokens, index, end, symbols);
        index = end + 1;
    }
    Some(index)
}

#[derive(Clone, Debug)]
struct PendingFunction {
    name: String,
    line: u32,
    body_start: usize,
    body_end: usize,
}

/// Analyze one C source file.
#[must_use]
pub fn analyze(source: &str) -> Analysis {
    let tokens = tokenize(source);
    let mut analysis = Analysis::default();
    let mut pending = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if tokens[index].kind == TokenKind::Directive {
            parse_define(&tokens[index], &mut analysis.symbols);
            index += 1;
            continue;
        }
        if tokens[index].is_ident("typedef") {
            index = scan_typedef_end(&tokens, index);
            continue;
        }
        if let Some(next) = parse_type(&tokens, index, &mut analysis.symbols) {
            index = next;
            continue;
        }
        let (end, terminator) = scan_declaration(&tokens, index);
        match terminator {
            Terminator::Brace => {
                if let Some((name, line)) = function_header(&tokens, index, end) {
                    let body_end = matching_brace(&tokens, end).unwrap_or(end);
                    analysis.symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Function,
                        line,
                    });
                    pending.push(PendingFunction {
                        name,
                        line,
                        body_start: end + 1,
                        body_end,
                    });
                    index = body_end + 1;
                } else {
                    record_declarators(&tokens, index, end, &mut analysis.symbols);
                    let close = matching_brace(&tokens, end).unwrap_or(end);
                    index = close + 1;
                }
            }
            Terminator::Semicolon => {
                record_declarators(&tokens, index, end, &mut analysis.symbols);
                index = end + 1;
            }
            Terminator::Close => index += 1,
            Terminator::Eof => break,
        }
    }

    let names = pending
        .iter()
        .map(|function| function.name.clone())
        .collect::<Vec<_>>();
    for function in pending {
        let mut calls = Vec::new();
        for position in function.body_start..function.body_end {
            if !tokens[position].is_identifier()
                || !tokens
                    .get(position + 1)
                    .is_some_and(|token| token.is_punct('('))
            {
                continue;
            }
            let name = &tokens[position].text;
            if names.iter().any(|candidate| candidate == name) && !calls.contains(name) {
                calls.push(name.clone());
            }
        }
        analysis.functions.push(Function {
            name: function.name,
            line: function.line,
            calls,
        });
    }

    for token in &tokens {
        if token.is_identifier() {
            analysis.references.push(Reference {
                name: token.text.clone(),
                line: token.line,
            });
        }
    }
    analysis
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
