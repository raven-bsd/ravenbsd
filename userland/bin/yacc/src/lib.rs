// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `yacc` parser generator.
//!
//! The tool builds LALR(1) tables (canonical LR(1) item sets merged by core), resolves
//! shift/reduce conflicts with `%left`/`%right`/`%nonassoc` and `%prec`, and emits C with
//! `yyparse`, `yylval`, `yynerrs` and `error`-token recovery.
//!
//! The bounded subset uses an `int` semantic value: `%union`, `%type`, mid-rule actions,
//! `%destructor`, `%locations` and `%expect` are explicit unsupported cases.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![cfg(not(target_os = "none"))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Associativity of a precedence declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    /// `%left`.
    Left,
    /// `%right`.
    Right,
    /// `%nonassoc`.
    Nonassoc,
}

/// One declared token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// Token name.
    pub name: String,
    /// Explicit token number, if the declaration supplied one.
    pub number: Option<i32>,
    /// Precedence level, higher binds tighter.
    pub precedence: Option<i32>,
    /// Associativity from the precedence declaration.
    pub assoc: Option<Assoc>,
}

/// One grammar symbol.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Symbol {
    /// A named symbol; resolved to a token or a nonterminal.
    Name(String),
    /// A character literal token.
    Char(u8),
}

/// One grammar rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Left-hand side nonterminal.
    pub lhs: String,
    /// Right-hand side.
    pub rhs: Vec<Symbol>,
    /// Action block, including braces; empty when omitted.
    pub action: String,
    /// `%prec` token.
    pub prec: Option<Symbol>,
    /// Source line.
    pub line: usize,
}

/// A parsed grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grammar {
    /// `%{ ... %}` code.
    pub prologue: String,
    /// Code after the second `%%`.
    pub epilogue: String,
    /// Declared tokens.
    pub tokens: Vec<Token>,
    /// Start symbol, if `%start` was used.
    pub start: Option<String>,
    /// Rules in source order.
    pub rules: Vec<Rule>,
}

/// A parser table action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Reject the token.
    Error,
    /// Shift the token and enter the state.
    Shift(usize),
    /// Reduce by the rule.
    Reduce(usize),
    /// Accept the input.
    Accept,
}

/// Generated parser tables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    /// Action table indexed by `[state][token number]`.
    pub action: Vec<Vec<Action>>,
    /// Goto table indexed by `[state][nonterminal]`; `None` is an error.
    pub goto: Vec<Vec<Option<usize>>>,
    /// Highest token number in the action table.
    pub max_token: i32,
    /// Number of nonterminals.
    pub nonterminals: usize,
    /// Number of shift/reduce conflicts.
    pub shift_reduce: usize,
    /// Number of reduce/reduce conflicts.
    pub reduce_reduce: usize,
}

/// A grammar parse or generation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// Source line, or 0.
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

/// Parse and compile a yacc grammar to C.
///
/// # Errors
/// Returns [`Error`] for a malformed grammar, an unsupported construct, an undefined symbol or
/// table construction failure.
pub fn compile(source: &str) -> Result<String, Error> {
    let grammar = Grammar::parse(source)?;
    compile_grammar(&grammar)
}

impl Grammar {
    /// Parse the declarations, rules and user-code sections.
    ///
    /// # Errors
    /// Returns [`Error`] with a source line for malformed input or an unsupported directive.
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
        let mut grammar = Self {
            prologue: String::new(),
            epilogue: String::new(),
            tokens: Vec::new(),
            start: None,
            rules: Vec::new(),
        };
        parse_declarations(&lines[..first], &mut grammar)?;
        let rules_end = second.unwrap_or(lines.len());
        let text = lines[first + 1..rules_end].join("\n");
        parse_rules(&text, first + 2, &mut grammar)?;
        if let Some(index) = second {
            grammar.epilogue = lines[index + 1..].join("\n");
        }
        if grammar.rules.is_empty() {
            return Err(Error::new(0, "no grammar rules"));
        }
        Ok(grammar)
    }
}

fn parse_declarations(lines: &[&str], grammar: &mut Grammar) -> Result<(), Error> {
    let mut index = 0usize;
    let mut precedence = 0i32;
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
            if !grammar.prologue.is_empty() {
                grammar.prologue.push('\n');
            }
            grammar.prologue.push_str(&code.join("\n"));
            continue;
        }
        let (directive, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(directive, rest)| (directive, rest.trim()));
        match directive {
            "%token" => parse_token_declaration(rest, number, grammar)?,
            "%left" | "%right" | "%nonassoc" => {
                precedence += 1;
                let assoc = match directive {
                    "%left" => Assoc::Left,
                    "%right" => Assoc::Right,
                    _ => Assoc::Nonassoc,
                };
                for text in rest.split_whitespace() {
                    let symbol = parse_symbol(text, number)?;
                    let name = match symbol {
                        Symbol::Name(name) => name,
                        Symbol::Char(byte) => format!("'{}'", byte as char),
                    };
                    let token = token_mut(grammar, &name);
                    token.precedence = Some(precedence);
                    token.assoc = Some(assoc);
                }
            }
            "%start" => {
                if rest.is_empty() || grammar.start.is_some() {
                    return Err(Error::new(number, "invalid %start declaration"));
                }
                grammar.start = Some(rest.to_string());
            }
            "%union" | "%type" | "%destructor" | "%locations" | "%expect" | "%define" => {
                return Err(Error::new(
                    number,
                    format!("unsupported yacc directive `{directive}`"),
                ));
            }
            other if other.starts_with('%') => {
                return Err(Error::new(
                    number,
                    format!("unsupported yacc directive `{other}`"),
                ));
            }
            _ => return Err(Error::new(number, "expected a declaration")),
        }
    }
    Ok(())
}

fn parse_token_declaration(rest: &str, number: usize, grammar: &mut Grammar) -> Result<(), Error> {
    let mut words = rest.split_whitespace().peekable();
    while let Some(word) = words.next() {
        if word.starts_with('<') {
            return Err(Error::new(number, "%token type tags are unsupported"));
        }
        let symbol = parse_symbol(word, number)?;
        let name = match symbol {
            Symbol::Name(name) => name,
            Symbol::Char(byte) => format!("'{}'", byte as char),
        };
        let explicit = words
            .peek()
            .and_then(|next| next.parse::<i32>().ok())
            .inspect(|_| {
                words.next();
            });
        if explicit == Some(0) || explicit == Some(1) {
            return Err(Error::new(number, "token numbers 0 and 1 are reserved"));
        }
        let token = token_mut(grammar, &name);
        if let Some(value) = explicit {
            token.number = Some(value);
        }
    }
    Ok(())
}

fn token_mut<'a>(grammar: &'a mut Grammar, name: &str) -> &'a mut Token {
    if let Some(index) = grammar.tokens.iter().position(|token| token.name == name) {
        return &mut grammar.tokens[index];
    }
    grammar.tokens.push(Token {
        name: name.to_string(),
        number: None,
        precedence: None,
        assoc: None,
    });
    grammar.tokens.last_mut().expect("just pushed")
}

fn parse_symbol(text: &str, line: usize) -> Result<Symbol, Error> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        let byte = parse_char(inner)
            .ok_or_else(|| Error::new(line, format!("bad character token `{text}`")))?;
        return Ok(Symbol::Char(byte));
    }
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return Err(Error::new(line, format!("bad symbol `{text}`")));
    }
    Ok(Symbol::Name(text.to_string()))
}

fn parse_char(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    if bytes.len() == 1 {
        return Some(bytes[0]);
    }
    if bytes.first() == Some(&b'\\') {
        return match bytes.get(1) {
            Some(b'n') => Some(b'\n'),
            Some(b't') => Some(b'\t'),
            Some(b'r') => Some(b'\r'),
            Some(b'0') => Some(0),
            Some(b'\\') => Some(b'\\'),
            Some(b'\'') => Some(b'\''),
            Some(b'"') => Some(b'"'),
            Some(byte) if bytes.len() == 2 => Some(*byte),
            _ => None,
        };
    }
    None
}

fn parse_rules(text: &str, line_base: usize, grammar: &mut Grammar) -> Result<(), Error> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut line = line_base;
    while index < bytes.len() {
        skip_space_and_comments(bytes, &mut index, &mut line);
        if index >= bytes.len() {
            break;
        }
        let lhs_line = line;
        let lhs = read_identifier(bytes, &mut index)
            .ok_or_else(|| Error::new(lhs_line, "expected a rule left-hand side"))?;
        skip_space_and_comments(bytes, &mut index, &mut line);
        if bytes.get(index) != Some(&b':') {
            return Err(Error::new(
                lhs_line,
                "expected `:` after the left-hand side",
            ));
        }
        index += 1;
        loop {
            let mut rhs = Vec::new();
            let mut action = String::new();
            let mut prec = None;
            let mut saw_action = false;
            loop {
                skip_space_and_comments(bytes, &mut index, &mut line);
                if index >= bytes.len() {
                    return Err(Error::new(lhs_line, "unterminated rule (missing `;`)"));
                }
                match bytes[index] {
                    b'|' | b';' => break,
                    b'{' => {
                        if saw_action {
                            return Err(Error::new(line, "two actions in one alternative"));
                        }
                        action = read_block(bytes, &mut index, &mut line)?;
                        saw_action = true;
                    }
                    b'%' => {
                        index += 1;
                        skip_space_and_comments(bytes, &mut index, &mut line);
                        let directive = read_identifier(bytes, &mut index)
                            .ok_or_else(|| Error::new(line, "expected `%prec`"))?;
                        if directive != "prec" {
                            return Err(Error::new(
                                line,
                                format!("unsupported rule directive `%{directive}`"),
                            ));
                        }
                        skip_space_and_comments(bytes, &mut index, &mut line);
                        let end = symbol_end(bytes, index);
                        let token = std::str::from_utf8(&bytes[index..end])
                            .map_err(|_| Error::new(line, "bad %prec token"))?;
                        prec = Some(parse_symbol(token, line)?);
                        index = end;
                    }
                    _ => {
                        if saw_action {
                            return Err(Error::new(line, "mid-rule actions are unsupported"));
                        }
                        let end = symbol_end(bytes, index);
                        let token = std::str::from_utf8(&bytes[index..end])
                            .map_err(|_| Error::new(line, "bad grammar symbol"))?;
                        if token.starts_with('%') {
                            return Err(Error::new(line, format!("unsupported `{token}`")));
                        }
                        rhs.push(parse_symbol(token, line)?);
                        index = end;
                    }
                }
            }
            grammar.rules.push(Rule {
                lhs: lhs.clone(),
                rhs,
                action,
                prec,
                line: lhs_line,
            });
            match bytes.get(index) {
                Some(b'|') => {
                    index += 1;
                }
                Some(b';') => {
                    index += 1;
                    break;
                }
                _ => return Err(Error::new(line, "expected `|` or `;`")),
            }
        }
    }
    Ok(())
}

fn skip_space_and_comments(bytes: &[u8], index: &mut usize, line: &mut usize) {
    while *index < bytes.len() {
        match bytes[*index] {
            b' ' | b'\t' | b'\r' => *index += 1,
            b'\n' => {
                *line += 1;
                *index += 1;
            }
            b'/' if bytes.get(*index + 1) == Some(&b'*') => {
                *index += 2;
                while *index + 1 < bytes.len() {
                    if bytes[*index] == b'\n' {
                        *line += 1;
                    }
                    if bytes[*index] == b'*' && bytes[*index + 1] == b'/' {
                        *index += 2;
                        break;
                    }
                    *index += 1;
                }
            }
            b'/' if bytes.get(*index + 1) == Some(&b'/') => {
                while *index < bytes.len() && bytes[*index] != b'\n' {
                    *index += 1;
                }
            }
            _ => break,
        }
    }
}

fn read_identifier(bytes: &[u8], index: &mut usize) -> Option<String> {
    let start = *index;
    let first = *bytes.get(*index)?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    *index += 1;
    while let Some(byte) = bytes.get(*index) {
        if byte.is_ascii_alphanumeric() || *byte == b'_' {
            *index += 1;
        } else {
            break;
        }
    }
    std::str::from_utf8(&bytes[start..*index])
        .ok()
        .map(str::to_string)
}

fn symbol_end(bytes: &[u8], start: usize) -> usize {
    if bytes.get(start) == Some(&b'\'') {
        let mut index = start + 1;
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index += 2;
                continue;
            }
            if bytes[index] == b'\'' {
                return index + 1;
            }
            index += 1;
        }
        return bytes.len();
    }
    let mut index = start;
    while index < bytes.len()
        && !matches!(
            bytes[index],
            b' ' | b'\t' | b'\r' | b'\n' | b':' | b'|' | b';' | b'{' | b'}' | b'%'
        )
    {
        index += 1;
    }
    index
}

fn read_block(bytes: &[u8], index: &mut usize, line: &mut usize) -> Result<String, Error> {
    let start = *index;
    let mut balance = 0i32;
    let mut quote: Option<u8> = None;
    while *index < bytes.len() {
        let byte = bytes[*index];
        if let Some(current) = quote {
            if byte == b'\\' {
                *index += 2;
                continue;
            }
            if byte == current {
                quote = None;
            }
            *index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'{' => balance += 1,
            b'}' => {
                balance -= 1;
                if balance == 0 {
                    *index += 1;
                    return std::str::from_utf8(&bytes[start..*index])
                        .map(str::to_string)
                        .map_err(|_| Error::new(*line, "action is not UTF-8"));
                }
            }
            b'\n' => *line += 1,
            b'/' if bytes.get(*index + 1) == Some(&b'*') => {
                *index += 2;
                while *index + 1 < bytes.len() {
                    if bytes[*index] == b'\n' {
                        *line += 1;
                    }
                    if bytes[*index] == b'*' && bytes[*index + 1] == b'/' {
                        *index += 2;
                        break;
                    }
                    *index += 1;
                }
                continue;
            }
            _ => {}
        }
        *index += 1;
    }
    Err(Error::new(*line, "unterminated action block"))
}

struct LrRule {
    lhs: Option<usize>,
    rhs: Vec<Sym>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sym {
    Term(i32),
    Nonterm(usize),
}

type Item = (usize, usize, i32);

struct Resolved {
    nonterminals: Vec<String>,
    rules: Vec<LrRule>,
    precedence: BTreeMap<i32, (i32, Assoc)>,
    rule_prec: Vec<Option<(i32, Assoc)>>,
    max_token: i32,
}

fn resolve(grammar: &Grammar) -> Result<Resolved, Error> {
    let mut nonterminals: Vec<String> = Vec::new();
    for rule in &grammar.rules {
        if !nonterminals.contains(&rule.lhs) {
            nonterminals.push(rule.lhs.clone());
        }
    }
    let start_name = grammar
        .start
        .clone()
        .unwrap_or_else(|| nonterminals.first().cloned().unwrap_or_default());
    let Some(start) = nonterminals.iter().position(|name| name == &start_name) else {
        return Err(Error::new(
            0,
            format!("start symbol `{start_name}` has no rule"),
        ));
    };

    let mut used_numbers: BTreeSet<i32> = BTreeSet::new();
    used_numbers.insert(0);
    used_numbers.insert(1);
    let mut explicit = BTreeMap::new();
    for token in &grammar.tokens {
        if let Some(number) = token.number {
            if !used_numbers.insert(number) {
                return Err(Error::new(0, format!("duplicate token number {number}")));
            }
            explicit.insert(token.name.clone(), number);
        }
    }
    let mut next_number = 258i32;
    let mut token_numbers: BTreeMap<String, i32> = BTreeMap::new();
    token_numbers.insert("$end".to_string(), 0);
    token_numbers.insert("error".to_string(), 1);
    for token in &grammar.tokens {
        if let Ok(Symbol::Char(byte)) = parse_symbol(&token.name, 0) {
            token_numbers.insert(token.name.clone(), i32::from(byte));
            used_numbers.insert(i32::from(byte));
            continue;
        }
        let number = if let Some(number) = explicit.get(&token.name) {
            *number
        } else {
            while used_numbers.contains(&next_number) {
                next_number += 1;
            }
            let number = next_number;
            used_numbers.insert(number);
            next_number += 1;
            number
        };
        token_numbers.insert(token.name.clone(), number);
    }
    for rule in &grammar.rules {
        for symbol in &rule.rhs {
            if let Symbol::Char(byte) = symbol {
                used_numbers.insert(i32::from(*byte));
            }
        }
        if let Some(Symbol::Char(byte)) = &rule.prec {
            used_numbers.insert(i32::from(*byte));
        }
    }
    // Precedence-only tokens that never appeared in %token.
    for token in &grammar.tokens {
        if let Ok(Symbol::Char(byte)) = parse_symbol(&token.name, 0) {
            used_numbers.insert(i32::from(byte));
        }
    }
    let mut precedence = BTreeMap::new();
    for token in &grammar.tokens {
        if let (Some(level), Some(assoc)) = (token.precedence, token.assoc)
            && let Some(number) = token_numbers.get(&token.name)
        {
            precedence.insert(*number, (level, assoc));
        }
    }
    let mut rules = Vec::new();
    rules.push(LrRule {
        lhs: None,
        rhs: vec![Sym::Nonterm(start)],
    });
    for rule in &grammar.rules {
        let lhs = nonterminals
            .iter()
            .position(|name| name == &rule.lhs)
            .ok_or_else(|| Error::new(rule.line, "unknown left-hand side"))?;
        let mut rhs = Vec::new();
        for symbol in &rule.rhs {
            rhs.push(
                resolve_symbol(symbol, &token_numbers, &nonterminals).ok_or_else(|| {
                    Error::new(
                        rule.line,
                        format!("undefined symbol `{}`", render_symbol(symbol)),
                    )
                })?,
            );
        }
        rules.push(LrRule {
            lhs: Some(lhs),
            rhs,
        });
    }
    let mut rule_prec = vec![None];
    for rule in &grammar.rules {
        let explicit = rule.prec.as_ref().and_then(|symbol| match symbol {
            Symbol::Char(byte) => precedence.get(&i32::from(*byte)).copied(),
            Symbol::Name(name) => token_numbers
                .get(name)
                .and_then(|number| precedence.get(number).copied()),
        });
        let implicit = rule.rhs.iter().rev().find_map(|symbol| match symbol {
            Symbol::Char(byte) => precedence.get(&i32::from(*byte)).copied(),
            Symbol::Name(name) => token_numbers
                .get(name)
                .and_then(|number| precedence.get(number).copied()),
        });
        rule_prec.push(explicit.or(implicit));
    }
    let max_token = used_numbers.iter().next_back().copied().unwrap_or(1).max(1);
    Ok(Resolved {
        nonterminals,
        rules,
        precedence,
        rule_prec,
        max_token,
    })
}

fn resolve_symbol(
    symbol: &Symbol,
    tokens: &BTreeMap<String, i32>,
    nonterminals: &[String],
) -> Option<Sym> {
    match symbol {
        Symbol::Char(byte) => Some(Sym::Term(i32::from(*byte))),
        Symbol::Name(name) => {
            if let Some(number) = tokens.get(name) {
                Some(Sym::Term(*number))
            } else {
                nonterminals
                    .iter()
                    .position(|item| item == name)
                    .map(Sym::Nonterm)
            }
        }
    }
}

fn render_symbol(symbol: &Symbol) -> String {
    match symbol {
        Symbol::Name(name) => name.clone(),
        Symbol::Char(byte) => format!("'{}'", *byte as char),
    }
}

fn first_sets(resolved: &Resolved) -> (Vec<BTreeSet<i32>>, Vec<bool>) {
    let count = resolved.nonterminals.len();
    let mut first = vec![BTreeSet::new(); count];
    let mut nullable = vec![false; count];
    let mut changed = true;
    while changed {
        changed = false;
        for rule in resolved.rules.iter().skip(1) {
            let Some(lhs) = rule.lhs else { continue };
            let mut all_nullable = true;
            for symbol in &rule.rhs {
                match symbol {
                    Sym::Term(token) => {
                        if first[lhs].insert(*token) {
                            changed = true;
                        }
                        all_nullable = false;
                        break;
                    }
                    Sym::Nonterm(index) => {
                        let additions: Vec<i32> = first[*index].iter().copied().collect();
                        for token in additions {
                            if first[lhs].insert(token) {
                                changed = true;
                            }
                        }
                        if !nullable[*index] {
                            all_nullable = false;
                            break;
                        }
                    }
                }
            }
            if all_nullable && !nullable[lhs] {
                nullable[lhs] = true;
                changed = true;
            }
        }
    }
    (first, nullable)
}

fn first_of_sequence(
    sequence: &[Sym],
    lookahead: i32,
    first: &[BTreeSet<i32>],
    nullable: &[bool],
) -> BTreeSet<i32> {
    let mut result = BTreeSet::new();
    for symbol in sequence {
        match symbol {
            Sym::Term(token) => {
                result.insert(*token);
                return result;
            }
            Sym::Nonterm(index) => {
                result.extend(first[*index].iter().copied());
                if !nullable[*index] {
                    return result;
                }
            }
        }
    }
    result.insert(lookahead);
    result
}

fn closure(
    items: &BTreeSet<Item>,
    rules: &[LrRule],
    first: &[BTreeSet<i32>],
    nullable: &[bool],
) -> BTreeSet<Item> {
    let mut result = items.clone();
    let mut queue: Vec<Item> = items.iter().copied().collect();
    while let Some((rule, dot, lookahead)) = queue.pop() {
        let Some(current) = rules.get(rule) else {
            continue;
        };
        let Some(Sym::Nonterm(target)) = current.rhs.get(dot) else {
            continue;
        };
        let rest = &current.rhs[dot + 1..];
        let lookaheads = first_of_sequence(rest, lookahead, first, nullable);
        for (index, candidate) in rules.iter().enumerate() {
            if candidate.lhs != Some(*target) {
                continue;
            }
            for lookahead in &lookaheads {
                if result.insert((index, 0, *lookahead)) {
                    queue.push((index, 0, *lookahead));
                }
            }
        }
    }
    result
}

fn goto(
    items: &BTreeSet<Item>,
    symbol: Sym,
    rules: &[LrRule],
    first: &[BTreeSet<i32>],
    nullable: &[bool],
) -> BTreeSet<Item> {
    let mut moved = BTreeSet::new();
    for (rule, dot, lookahead) in items {
        let Some(current) = rules.get(*rule) else {
            continue;
        };
        if current.rhs.get(*dot) == Some(&symbol) {
            moved.insert((*rule, dot + 1, *lookahead));
        }
    }
    if moved.is_empty() {
        moved
    } else {
        closure(&moved, rules, first, nullable)
    }
}

/// Build the LALR(1) tables for a parsed grammar.
///
/// # Errors
/// Returns [`Error`] for an undefined grammar structure or a table overflow.
pub fn tables(grammar: &Grammar) -> Result<Tables, Error> {
    let resolved = resolve(grammar)?;
    let (first, nullable) = first_sets(&resolved);
    let mut states: Vec<BTreeSet<Item>> = Vec::new();
    let mut state_map: BTreeMap<BTreeSet<Item>, usize> = BTreeMap::new();
    let start = closure(
        &BTreeSet::from([(0usize, 0usize, 0i32)]),
        &resolved.rules,
        &first,
        &nullable,
    );
    state_map.insert(start.clone(), 0);
    states.push(start);
    let mut index = 0usize;
    while index < states.len() {
        let mut symbols = BTreeSet::new();
        for (rule, dot, _) in &states[index] {
            if let Some(symbol) = resolved.rules[*rule].rhs.get(*dot) {
                symbols.insert(*symbol);
            }
        }
        for symbol in symbols {
            let target = goto(&states[index], symbol, &resolved.rules, &first, &nullable);
            if target.is_empty() {
                continue;
            }
            if !state_map.contains_key(&target) {
                let next = states.len();
                state_map.insert(target.clone(), next);
                states.push(target);
            }
        }
        index += 1;
    }
    // Merge canonical states by LR(0) core to obtain LALR(1) states.
    let mut cores: BTreeMap<BTreeSet<(usize, usize)>, usize> = BTreeMap::new();
    let mut merged_index = Vec::with_capacity(states.len());
    for state in &states {
        let core: BTreeSet<(usize, usize)> =
            state.iter().map(|(rule, dot, _)| (*rule, *dot)).collect();
        let next_index = cores.len();
        let next = *cores.entry(core).or_insert(next_index);
        merged_index.push(next);
    }
    let merged_count = cores.len();
    let mut merged: Vec<BTreeSet<Item>> = vec![BTreeSet::new(); merged_count];
    for (index, state) in states.iter().enumerate() {
        merged[merged_index[index]].extend(state.iter().copied());
    }
    let mut transitions: BTreeMap<(usize, Sym), usize> = BTreeMap::new();
    for (index, state) in states.iter().enumerate() {
        let mut symbols = BTreeSet::new();
        for (rule, dot, _) in state {
            if let Some(symbol) = resolved.rules[*rule].rhs.get(*dot) {
                symbols.insert(*symbol);
            }
        }
        for symbol in symbols {
            let target = goto(state, symbol, &resolved.rules, &first, &nullable);
            if let Some(next) = state_map.get(&target) {
                let from = merged_index[index];
                let to = merged_index[*next];
                if let Some(existing) = transitions.insert((from, symbol), to)
                    && existing != to
                {
                    return Err(Error::new(0, "inconsistent LALR transition"));
                }
            }
        }
    }
    let terminals = resolved.max_token as usize + 1;
    let nonterminal_count = resolved.nonterminals.len();
    let mut action = vec![vec![Action::Error; terminals]; merged_count];
    let mut goto_table = vec![vec![None; nonterminal_count]; merged_count];
    let mut shift_reduce = 0usize;
    let mut reduce_reduce = 0usize;
    for (state_index, items) in merged.iter().enumerate() {
        for (rule_index, dot, lookahead) in items {
            let rule = &resolved.rules[*rule_index];
            if let Some(symbol) = rule.rhs.get(*dot) {
                match symbol {
                    Sym::Term(token) => {
                        if let Some(target) = transitions.get(&(state_index, *symbol)) {
                            set_action(
                                &mut action,
                                state_index,
                                *token,
                                Action::Shift(*target),
                                &resolved,
                                &mut shift_reduce,
                                &mut reduce_reduce,
                            );
                        }
                    }
                    Sym::Nonterm(index) => {
                        if let Some(target) = transitions.get(&(state_index, *symbol)) {
                            goto_table[state_index][*index] = Some(*target);
                        }
                    }
                }
            } else if *rule_index == 0 {
                if *lookahead == 0 {
                    action[state_index][0] = Action::Accept;
                }
            } else {
                set_action(
                    &mut action,
                    state_index,
                    *lookahead,
                    Action::Reduce(*rule_index),
                    &resolved,
                    &mut shift_reduce,
                    &mut reduce_reduce,
                );
            }
        }
    }
    Ok(Tables {
        action,
        goto: goto_table,
        max_token: resolved.max_token,
        nonterminals: nonterminal_count,
        shift_reduce,
        reduce_reduce,
    })
}

fn set_action(
    table: &mut [Vec<Action>],
    state: usize,
    token: i32,
    proposed: Action,
    resolved: &Resolved,
    shift_reduce: &mut usize,
    reduce_reduce: &mut usize,
) {
    let token_index = token as usize;
    if token_index >= table[state].len() {
        return;
    }
    let existing = table[state][token_index];
    let chosen = match (existing, proposed) {
        (Action::Error, action) => action,
        (action, Action::Error) => action,
        (Action::Accept, _) | (_, Action::Accept) => Action::Accept,
        (Action::Shift(left), Action::Shift(right)) if left == right => existing,
        (Action::Shift(_), Action::Shift(_)) => existing,
        (Action::Shift(shift), Action::Reduce(rule)) => {
            match precedence_choice(token, rule, resolved) {
                Some(Action::Reduce(_)) => Action::Reduce(rule),
                Some(Action::Shift(_)) => Action::Shift(shift),
                Some(Action::Error) => Action::Error,
                _ => {
                    *shift_reduce += 1;
                    Action::Shift(shift)
                }
            }
        }
        (Action::Reduce(rule), Action::Shift(shift)) => {
            match precedence_choice(token, rule, resolved) {
                Some(Action::Reduce(_)) => Action::Reduce(rule),
                Some(Action::Shift(_)) => Action::Shift(shift),
                Some(Action::Error) => Action::Error,
                _ => {
                    *shift_reduce += 1;
                    Action::Shift(shift)
                }
            }
        }
        (Action::Reduce(left), Action::Reduce(right)) => {
            *reduce_reduce += 1;
            if left <= right {
                Action::Reduce(left)
            } else {
                Action::Reduce(right)
            }
        }
    };
    table[state][token_index] = chosen;
}

fn precedence_choice(token: i32, rule: usize, resolved: &Resolved) -> Option<Action> {
    let token_prec = resolved.precedence.get(&token)?;
    let rule_prec = (*resolved.rule_prec.get(rule)?)?;
    if rule_prec.0 > token_prec.0 {
        return Some(Action::Reduce(rule));
    }
    if rule_prec.0 < token_prec.0 {
        return Some(Action::Shift(0));
    }
    match token_prec.1 {
        Assoc::Left => Some(Action::Reduce(rule)),
        Assoc::Right => Some(Action::Shift(0)),
        Assoc::Nonassoc => Some(Action::Error),
    }
}

/// Emit the C parser (and optionally the header) for a parsed grammar.
///
/// # Errors
/// Returns [`Error`] for table construction or an unsupported action reference.
pub fn compile_grammar(grammar: &Grammar) -> Result<String, Error> {
    let resolved = resolve(grammar)?;
    let tables = tables(grammar)?;
    emit(grammar, &resolved, &tables)
}

/// Return the generated `y.tab.h` token header.
///
/// # Errors
/// Returns [`Error`] when the grammar cannot be resolved.
pub fn header(grammar: &Grammar) -> Result<String, Error> {
    let _resolved = resolve(grammar)?;
    let numbers = token_numbers_public(grammar);
    let mut output = String::new();
    output.push_str("#ifndef YY_TAB_H\n#define YY_TAB_H\n\n");
    for (name, number) in &numbers {
        let _ = writeln!(output, "#define {name} {number}");
    }
    output.push_str("\n#ifndef YYSTYPE\ntypedef int YYSTYPE;\n#endif\n");
    output.push_str("extern YYSTYPE yylval;\nint yyparse(void);\n\n#endif\n");
    Ok(output)
}

fn emit(grammar: &Grammar, resolved: &Resolved, tables: &Tables) -> Result<String, Error> {
    let numbers = token_numbers_public(grammar);
    let mut output = String::new();
    output.push_str("/* Generated by RavenBSD yacc. */\n");
    output.push_str("#include <stdio.h>\n#include <stdlib.h>\n\n");
    if !grammar.prologue.is_empty() {
        output.push_str(&grammar.prologue);
        output.push('\n');
    }
    for (name, number) in &numbers {
        let _ = writeln!(output, "#define {name} {number}");
    }
    output.push_str("\n#ifndef YYSTYPE\ntypedef int YYSTYPE;\n#endif\n");
    output.push_str("YYSTYPE yylval;\nint yynerrs = 0;\nint yydebug = 0;\n");
    let _ = writeln!(output, "#define YYMAXTOKEN {}", tables.max_token);
    output
        .push_str("#define YYEMPTY (-2)\n#define YYERRTOKEN 1\n#define YY_ACTION_ERROR (-32768)\n");
    output.push_str("#define yyerrok (yyerrflag = 0)\n#define yyclearin (yychar = YYEMPTY)\n#define YYERROR goto yyerrorlab\n");
    output.push_str("int yylex(void);\nvoid yyerror(const char *);\n\n");
    output.push_str("static const short yy_action[][");
    let _ = writeln!(output, "{}] = {{", tables.max_token as usize + 1);
    for row in &tables.action {
        output.push_str("    {");
        for (index, action) in row.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            let value = match action {
                Action::Error => "-32768".to_string(),
                Action::Shift(state) => (state + 1).to_string(),
                Action::Reduce(rule) => format!("-{}", rule + 1),
                Action::Accept => "0".to_string(),
            };
            output.push_str(&value);
        }
        output.push_str("},\n");
    }
    output.push_str("};\n\n");
    let _ = writeln!(
        output,
        "static const short yy_goto[][{}] = {{",
        tables.nonterminals
    );
    for row in &tables.goto {
        output.push_str("    {");
        for (index, state) in row.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push_str(&state.map_or("0".to_string(), |value| (value + 1).to_string()));
        }
        output.push_str("},\n");
    }
    output.push_str("};\n\n");
    output.push_str("static const short yyr1[] = {");
    for (index, rule) in resolved.rules.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push_str(&rule.lhs.unwrap_or(0).to_string());
    }
    output.push_str("};\n");
    output.push_str("static const short yyr2[] = {");
    for (index, rule) in resolved.rules.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        output.push_str(&rule.rhs.len().to_string());
    }
    output.push_str("};\n\n");
    output.push_str("int yyparse(void) {\n");
    output.push_str("    int yystate = 0;\n    int yychar = YYEMPTY;\n    int yyerrflag = 0;\n");
    output.push_str("    int yyss[10000];\n    YYSTYPE yyvs[10000];\n    int yyssp = 0;\n");
    output.push_str("    yyss[0] = 0;\n    for (;;) {\n");
    output.push_str(
        "        if (yychar == YYEMPTY) { yychar = yylex(); if (yychar < 0) yychar = 0; }\n",
    );
    output.push_str("        if (yychar > YYMAXTOKEN) { yyerror(\"invalid token\"); return 1; }\n");
    output.push_str("        short yyact = yy_action[yystate][yychar];\n");
    output.push_str("        if (yyact == 0) return 0;\n");
    output.push_str("        if (yyact > 0) {\n");
    output.push_str(
        "            if (yyssp + 1 >= 10000) { yyerror(\"stack overflow\"); return 2; }\n",
    );
    output.push_str("            yystate = yyact - 1;\n            yyssp++;\n            yyss[yyssp] = yystate;\n            yyvs[yyssp] = yylval;\n            yychar = YYEMPTY;\n            continue;\n        }\n");
    output.push_str("        if (yyact == YY_ACTION_ERROR) {\n        if (0) goto yyerrorlab;\n        yyerrorlab:\n");
    output.push_str("            yyerror(\"syntax error\");\n            yynerrs++;\n");
    output.push_str("            if (yyerrflag == 0) {\n");
    output.push_str("                if (yychar == YYEMPTY) { yychar = yylex(); if (yychar < 0) yychar = 0; }\n");
    output.push_str("                yyerrflag = 3;\n            }\n");
    output.push_str("            for (;;) {\n");
    output.push_str("                short err = yy_action[yystate][YYERRTOKEN];\n");
    output.push_str("                if (err > 0) { yystate = err - 1; yyssp++; yyss[yyssp] = yystate; yyvs[yyssp] = yylval; break; }\n");
    output.push_str("                if (yyssp == 0) return 1;\n                yyssp--;\n                yystate = yyss[yyssp];\n");
    output.push_str("            }\n");
    output.push_str("            while (yyerrflag > 0) {\n");
    output.push_str("                if (yychar == YYEMPTY) { yychar = yylex(); if (yychar < 0) yychar = 0; }\n");
    output.push_str("                if (yychar == 0) return 1;\n");
    output.push_str("                short next = yy_action[yystate][yychar];\n");
    output
        .push_str("                if (next == YY_ACTION_ERROR) { yychar = YYEMPTY; continue; }\n");
    output.push_str("                yyerrflag--;\n                break;\n            }\n            continue;\n");
    output.push_str("        }\n");
    output.push_str(
        "        {\n            int yyrule = -yyact - 1;\n            int yylen = yyr2[yyrule];\n",
    );
    output.push_str("            YYSTYPE yyval = yylen ? yyvs[yyssp - yylen + 1] : 0;\n");
    output.push_str("            switch (yyrule) {\n");
    for (index, rule) in grammar.rules.iter().enumerate() {
        let rule_index = index + 1;
        let translated = translate_action(&rule.action, rule.rhs.len(), rule.line)?;
        let _ = writeln!(output, "            case {rule_index}:");
        output.push_str("                ");
        output.push_str(&translated);
        output.push_str("\n                break;\n");
    }
    output.push_str(
        "            }\n            yyssp -= yylen;\n            yystate = yyss[yyssp];\n",
    );
    output.push_str("            short next = yy_goto[yystate][yyr1[yyrule]];\n");
    output.push_str("            if (next == 0) { yyerror(\"internal goto\"); return 2; }\n");
    output.push_str("            yystate = next - 1;\n            yyssp++;\n            yyss[yyssp] = yystate;\n            yyvs[yyssp] = yyval;\n        }\n    }\n}\n\n");
    if !grammar.prologue.contains("yyerror") && !grammar.epilogue.contains("yyerror") {
        output.push_str(
            "void yyerror(const char *message) { fprintf(stderr, \"%s\\n\", message); }\n\n",
        );
    }
    if !grammar.epilogue.is_empty() {
        output.push_str(&grammar.epilogue);
        output.push('\n');
    }
    Ok(output)
}

fn token_numbers_public(grammar: &Grammar) -> Vec<(String, i32)> {
    let mut used: BTreeSet<i32> = BTreeSet::from([0, 1]);
    let mut explicit = BTreeMap::new();
    for token in &grammar.tokens {
        if let Some(number) = token.number {
            used.insert(number);
            explicit.insert(token.name.clone(), number);
        }
    }
    let mut next = 258;
    let mut output = Vec::new();
    for token in &grammar.tokens {
        let number = if let Some(number) = explicit.get(&token.name) {
            *number
        } else {
            while used.contains(&next) {
                next += 1;
            }
            used.insert(next);
            let number = next;
            next += 1;
            number
        };
        if token.name.starts_with('\'') {
            continue;
        }
        output.push((token.name.clone(), number));
    }
    output
}

fn translate_action(action: &str, length: usize, line: usize) -> Result<String, Error> {
    let bytes = action.as_bytes();
    let mut output = String::new();
    let mut index = 0usize;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(current) = quote {
            output.push(byte as char);
            if byte == b'\\'
                && let Some(next) = bytes.get(index + 1)
            {
                output.push(*next as char);
                index += 2;
                continue;
            }
            if byte == current {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            output.push(byte as char);
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            output.push_str("/*");
            index += 2;
            while index + 1 < bytes.len() {
                output.push(bytes[index] as char);
                if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                    output.push('/');
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                output.push(bytes[index] as char);
                index += 1;
            }
            continue;
        }
        if byte == b'$' {
            if bytes.get(index + 1) == Some(&b'$') {
                output.push_str("yyval");
                index += 2;
                continue;
            }
            if bytes.get(index + 1) == Some(&b'<') {
                return Err(Error::new(line, "$<field> semantic values are unsupported"));
            }
            let mut end = index + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > index + 1 {
                let text = std::str::from_utf8(&bytes[index + 1..end]).unwrap_or("0");
                let number = text.parse::<usize>().unwrap_or(0);
                if number == 0 || number > length {
                    return Err(Error::new(
                        line,
                        "action references a value outside the rule",
                    ));
                }
                let _ = write!(output, "yyvs[yyssp - {} + {}]", length, number);
                index = end;
                continue;
            }
        }
        output.push(byte as char);
        index += 1;
    }
    Ok(output)
}

/// Return the number of rules and the conflict counts for diagnostics.
///
/// # Errors
/// Returns [`Error`] when the grammar cannot be resolved.
pub fn statistics(grammar: &Grammar) -> Result<(usize, usize, usize), Error> {
    let tables = tables(grammar)?;
    Ok((
        grammar.rules.len(),
        tables.shift_reduce,
        tables.reduce_reduce,
    ))
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
