// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Recursive-descent parser for the supported pf.conf subset.
//!
//! Grammar source: FreeBSD `sbin/pfctl/parse.y` at `133344405f` (BSD-2-Clause),
//! nonterminals `ruleset`, `include`, `option`, `varset`, `varstring`,
//! `tabledef`, `pfrule`, `natrule`, `binatrule`, `rdrrule`, `fromto`, `from`,
//! `to`, `ipportspec`, `ipspec`, `xhost`, `host`, `portspec`, `action`, `dir`,
//! `logquick`, `interface`, `af`, `proto`. The implementation is first-party
//! Rust; the divergences (single protocol, numeric ports, no hostnames, no
//! address ranges, no nested anchors) are listed in `docs/pfctl.md`.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::ast::{
    Action, AddrItem, AddrPortSpec, AddressFamily, Config, Direction, FlagsSpec, HostAddr,
    IfaceRef, NatKind, NatRule, OptionDirective, Policy, PolicyValue, PoolOption, PortSpec,
    Protocol, Redirect, Rule, StateMode, StatePolicyValue, Statement, TableDef, TableEntry,
    TableFlag, is_address_literal,
};
use crate::lexer::{Spanned, Token, tokenize};

/// Maximum include nesting depth.
pub const MAX_INCLUDE_DEPTH: usize = 8;
/// Maximum macro expansion depth.
pub const MAX_MACRO_DEPTH: usize = 8;

/// Reads include files for the parser and the CLI.
pub trait FileLoader {
    /// Read `path` as UTF-8 text.
    fn read(&self, path: &str) -> Result<String, String>;
}

/// A parse or validation error with the offending source position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Source file, when the caller named one.
    pub file: Option<String>,
    /// 1-based line number.
    pub line: u32,
    /// Human-readable message.
    pub message: String,
}

impl ParseError {
    /// Render as `file:line: message` (or `line: message` without a file name).
    #[must_use]
    pub fn display(&self) -> String {
        match &self.file {
            Some(file) => format!("{file}:{}: {}", self.line, self.message),
            None => format!("{}: {}", self.line, self.message),
        }
    }
}

/// Parser inputs beyond the text itself.
#[derive(Default)]
pub struct ParseOptions<'a> {
    /// Source name for error messages.
    pub source: Option<&'a str>,
    /// Include loader; without one, `include` stays a [`Statement::Include`].
    pub loader: Option<&'a dyn FileLoader>,
    /// Predefined macros (the `-D name=value` CLI form).
    pub macros: Vec<(String, String)>,
}

/// Parse one pf.conf text without expanding includes.
pub fn parse_config(text: &str) -> Result<Config, ParseError> {
    parse_with(text, ParseOptions::default())
}

/// Parse with a source name and an include loader (includes expand in place).
pub fn parse_config_with_loader(
    text: &str,
    source: &str,
    loader: &dyn FileLoader,
) -> Result<Config, ParseError> {
    parse_with(
        text,
        ParseOptions {
            source: Some(source),
            loader: Some(loader),
            macros: Vec::new(),
        },
    )
}

/// Parse with explicit options.
pub fn parse_with(text: &str, options: ParseOptions<'_>) -> Result<Config, ParseError> {
    let file = options.source.map(ToString::to_string);
    let tokens = tokenize(text).map_err(|error| ParseError {
        file: file.clone(),
        line: error.line,
        message: error.message,
    })?;
    let mut macros = BTreeMap::new();
    for (name, value) in options.macros {
        let _ = macros.insert(name, value);
    }
    let tokens = expand_macros(tokens, &mut macros, &file)?;
    let mut parser = Parser {
        tokens,
        index: 0,
        macros,
        loader: options.loader,
        file,
        depth: 0,
        block_policy: Policy::Drop,
    };
    parser.parse_statements()
}

/// Expand `$macro` references in definition order.
///
/// FreeBSD's lexer expands macros textually while parsing; this pre-pass does the
/// same over the whole stream, recording definitions as it walks and resolving
/// every `$name` against the definitions seen so far plus the `-D` macros.
fn expand_macros(
    tokens: Vec<Spanned>,
    macros: &mut BTreeMap<String, String>,
    file: &Option<String>,
) -> Result<Vec<Spanned>, ParseError> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut index = 0;
    let mut statement_start = true;
    while index < tokens.len() {
        let spanned = tokens[index].clone();
        match &spanned.token {
            Token::Eof => {
                out.push(spanned);
                index += 1;
            }
            Token::Newline => {
                statement_start = true;
                out.push(spanned);
                index += 1;
            }
            Token::Word(name)
                if statement_start
                    && !name.starts_with('$')
                    && matches!(
                        tokens.get(index + 1).map(|next| &next.token),
                        Some(Token::Eq)
                    ) =>
            {
                out.push(spanned.clone());
                out.push(tokens[index + 1].clone());
                let mut value = String::new();
                index += 2;
                while index < tokens.len()
                    && !matches!(tokens[index].token, Token::Newline | Token::Eof)
                {
                    token_text(&tokens[index].token, &mut value);
                    out.push(tokens[index].clone());
                    index += 1;
                }
                let _ = macros.insert(name.clone(), value);
                statement_start = false;
            }
            Token::Word(word) if word.starts_with('$') => {
                let expanded = resolve_macro(word, macros, file, spanned.line, &mut Vec::new())?;
                out.extend(expanded);
                index += 1;
                statement_start = false;
            }
            _ => {
                out.push(spanned);
                index += 1;
                statement_start = false;
            }
        }
    }
    Ok(out)
}

fn resolve_macro(
    word: &str,
    macros: &BTreeMap<String, String>,
    file: &Option<String>,
    line: u32,
    stack: &mut Vec<String>,
) -> Result<Vec<Spanned>, ParseError> {
    let name = &word[1..];
    if name.is_empty() {
        return Err(ParseError {
            file: file.clone(),
            line,
            message: String::from("empty macro reference"),
        });
    }
    let Some(value) = macros.get(name) else {
        return Err(ParseError {
            file: file.clone(),
            line,
            message: format!("macro `${name}' is not defined"),
        });
    };
    if stack.len() >= MAX_MACRO_DEPTH {
        return Err(ParseError {
            file: file.clone(),
            line,
            message: format!("macro `${name}' expansion is too deep"),
        });
    }
    if stack.iter().any(|entry| entry == name) {
        return Err(ParseError {
            file: file.clone(),
            line,
            message: format!("macro `${name}' references itself"),
        });
    }
    let tokens = tokenize(value).map_err(|error| ParseError {
        file: file.clone(),
        line,
        message: format!("macro `${name}': {}", error.message),
    })?;
    stack.push(String::from(name));
    let mut out = Vec::new();
    for spanned in tokens {
        match &spanned.token {
            Token::Eof => {}
            Token::Word(word) if word.starts_with('$') => {
                out.extend(resolve_macro(word, macros, file, line, stack)?);
            }
            token => out.push(Spanned {
                token: token.clone(),
                line,
            }),
        }
    }
    stack.pop();
    Ok(out)
}

fn token_text(token: &Token, out: &mut String) {
    match token {
        Token::Word(word) | Token::Str(word) => out.push_str(word),
        Token::LBrace => out.push('{'),
        Token::RBrace => out.push('}'),
        Token::Lt => out.push('<'),
        Token::Gt => out.push('>'),
        Token::Bang => out.push('!'),
        Token::Eq => out.push('='),
        Token::Ne => out.push_str("!="),
        Token::Le => out.push_str("<="),
        Token::Ge => out.push_str(">="),
        Token::Arrow => out.push_str("->"),
        Token::Comma => out.push(','),
        Token::LParen => out.push('('),
        Token::RParen => out.push(')'),
        Token::Newline | Token::Eof => {}
    }
}

struct Parser<'a> {
    tokens: Vec<Spanned>,
    index: usize,
    macros: BTreeMap<String, String>,
    loader: Option<&'a dyn FileLoader>,
    file: Option<String>,
    depth: usize,
    block_policy: Policy,
}

impl Parser<'_> {
    fn err(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            file: self.file.clone(),
            line: self.peek().line,
            message: message.into(),
        }
    }

    fn err_at(&self, line: u32, message: impl Into<String>) -> ParseError {
        ParseError {
            file: self.file.clone(),
            line,
            message: message.into(),
        }
    }

    fn peek(&self) -> &Spanned {
        self.tokens.get(self.index).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("the lexer always ends the stream with Eof")
        })
    }

    fn peek_token(&self) -> &Token {
        &self.peek().token
    }

    fn next(&mut self) -> Spanned {
        let token = self.peek().clone();
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
        token
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_token(), Token::Eof)
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_token(), Token::Newline) {
            self.next();
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        match self.peek_token() {
            Token::Word(current) if current == word => {
                self.next();
                true
            }
            _ => false,
        }
    }

    fn eat_punct(&mut self, token: &Token) -> bool {
        if self.peek_token() == token {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, what: &str) -> Result<String, ParseError> {
        match self.peek_token() {
            Token::Word(word) => {
                let word = word.clone();
                self.next();
                Ok(word)
            }
            other => Err(self.err(format!("expected {what}, found {other:?}"))),
        }
    }

    fn expect_string(&mut self, what: &str) -> Result<String, ParseError> {
        match self.peek_token() {
            Token::Str(text) | Token::Word(text) => {
                let text = text.clone();
                self.next();
                Ok(text)
            }
            other => Err(self.err(format!("expected {what}, found {other:?}"))),
        }
    }

    fn expect_punct(&mut self, token: &Token, what: &str) -> Result<(), ParseError> {
        if self.eat_punct(token) {
            Ok(())
        } else {
            Err(self.err(format!("expected {what}, found {:?}", self.peek_token())))
        }
    }

    fn expect_end(&mut self) -> Result<(), ParseError> {
        match self.peek_token() {
            Token::Newline | Token::Eof => Ok(()),
            other => Err(self.err(format!("unexpected token {other:?} after the statement"))),
        }
    }

    fn parse_statements(&mut self) -> Result<Config, ParseError> {
        let mut statements = Vec::new();
        loop {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }
            self.parse_statement(&mut statements)?;
            self.expect_end()?;
        }
        Ok(Config { statements })
    }

    fn parse_statement(&mut self, statements: &mut Vec<Statement>) -> Result<(), ParseError> {
        let line = self.peek().line;
        let word = self.expect_word("a directive")?;
        match word.as_str() {
            "include" => {
                let path = self.expect_string("an include path")?;
                match self.loader {
                    Some(loader) => {
                        if self.depth >= MAX_INCLUDE_DEPTH {
                            return Err(self.err("include nesting is too deep"));
                        }
                        let text = loader.read(&path).map_err(|error| {
                            self.err(format!("cannot read include {path}: {error}"))
                        })?;
                        let tokens = tokenize(&text).map_err(|error| ParseError {
                            file: Some(path.clone()),
                            line: error.line,
                            message: error.message,
                        })?;
                        let mut child_macros = self.macros.clone();
                        let tokens = expand_macros(tokens, &mut child_macros, &Some(path.clone()))?;
                        let mut child = Parser {
                            tokens,
                            index: 0,
                            macros: child_macros,
                            loader: self.loader,
                            file: Some(path),
                            depth: self.depth + 1,
                            block_policy: self.block_policy,
                        };
                        let config = child.parse_statements()?;
                        self.macros = child.macros;
                        self.block_policy = child.block_policy;
                        statements.extend(config.statements);
                    }
                    None => statements.push(Statement::Include(path)),
                }
            }
            "table" => statements.push(Statement::Table(self.parse_table()?)),
            "set" => statements.push(Statement::Option(self.parse_option()?)),
            "anchor" => {
                let name = self.expect_string("an anchor name")?;
                if name.is_empty() {
                    return Err(self.err("anchor name must not be empty"));
                }
                statements.push(Statement::Anchor(name));
            }
            "pass" => {
                let rule = self.parse_filter_rule(Action::Pass)?;
                statements.push(Statement::Rule(rule));
            }
            "match" => {
                let rule = self.parse_filter_rule(Action::Match)?;
                statements.push(Statement::Rule(rule));
            }
            "block" => {
                let policy = if self.eat_word("drop") {
                    Policy::Drop
                } else if self.eat_word("return") {
                    Policy::Return
                } else {
                    self.block_policy
                };
                let rule = self.parse_filter_rule(Action::Block(policy))?;
                statements.push(Statement::Rule(rule));
            }
            "scrub" => {
                let rule = self.parse_filter_rule(Action::Scrub)?;
                statements.push(Statement::Rule(rule));
            }
            "nat" => statements.push(Statement::Nat(self.parse_nat_rule(NatKind::Nat)?)),
            "rdr" => statements.push(Statement::Nat(self.parse_nat_rule(NatKind::Rdr)?)),
            "binat" => statements.push(Statement::Nat(self.parse_nat_rule(NatKind::Binat)?)),
            "no" => {
                let kind = self.expect_word("`scrub`, `nat`, `rdr` or `binat`")?;
                match kind.as_str() {
                    "scrub" => {
                        let rule = self.parse_filter_rule(Action::NoScrub)?;
                        statements.push(Statement::Rule(rule));
                    }
                    "nat" => statements.push(Statement::Nat(self.parse_nat_rule(NatKind::NoNat)?)),
                    "rdr" => statements.push(Statement::Nat(self.parse_nat_rule(NatKind::NoRdr)?)),
                    "binat" => {
                        statements.push(Statement::Nat(self.parse_nat_rule(NatKind::NoBinat)?))
                    }
                    other => return Err(self.err(format!("unknown `no {other}' directive"))),
                }
            }
            _ => {
                if !self.eat_punct(&Token::Eq) {
                    return Err(self.err_at(
                        line,
                        format!("unknown directive `{word}' (a macro definition needs `=')"),
                    ));
                }
                let value = self.parse_macro_value()?;
                statements.push(Statement::Macro {
                    name: word.clone(),
                    value: value.clone(),
                });
                let _ = self.macros.insert(word, value);
            }
        }
        Ok(())
    }

    fn parse_macro_value(&mut self) -> Result<String, ParseError> {
        let mut value = String::new();
        while !matches!(self.peek_token(), Token::Newline | Token::Eof) {
            let token = self.next().token;
            token_text(&token, &mut value);
        }
        if value.is_empty() {
            return Err(self.err("macro value must not be empty"));
        }
        Ok(value)
    }

    fn parse_table(&mut self) -> Result<TableDef, ParseError> {
        self.expect_punct(&Token::Lt, "`<`")?;
        let name = self.expect_word("a table name")?;
        self.expect_punct(&Token::Gt, "`>`")?;
        let mut flags = Vec::new();
        loop {
            if self.eat_word("const") {
                flags.push(TableFlag::Const);
            } else if self.eat_word("persist") {
                flags.push(TableFlag::Persist);
            } else if self.eat_word("counters") {
                flags.push(TableFlag::Counters);
            } else {
                break;
            }
        }
        let mut entries = Vec::new();
        if self.eat_word("file") {
            let path = self.expect_string("a table file path")?;
            entries.push(TableEntry::File(path));
        } else if self.eat_punct(&Token::LBrace) {
            loop {
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                if self.eat_word("file") {
                    let path = self.expect_string("a table file path")?;
                    entries.push(TableEntry::File(path));
                } else {
                    let negative = self.eat_punct(&Token::Bang);
                    let word = self.expect_word("an address")?;
                    if !is_address_literal(&word) {
                        return Err(self.err(format!("invalid address `{word}' in table <{name}>")));
                    }
                    entries.push(TableEntry::Address {
                        negative,
                        address: HostAddr::Literal(word),
                    });
                }
                self.skip_newlines();
                if self.eat_punct(&Token::Comma) {
                    continue;
                }
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                // The donor prints table bodies space-separated (`{ a b }`), so a
                // space between entries is a terminator just like a comma.
                if matches!(self.peek_token(), Token::Bang | Token::Word(_)) {
                    continue;
                }
                return Err(self.err("expected `,` or `}` in the table body"));
            }
        }
        Ok(TableDef {
            name,
            flags,
            entries,
        })
    }

    fn parse_option(&mut self) -> Result<OptionDirective, ParseError> {
        let name = self.expect_word("an option name")?;
        match name.as_str() {
            "block-policy" => {
                let value = self.parse_policy_value()?;
                self.block_policy = match value {
                    PolicyValue::Drop => Policy::Drop,
                    PolicyValue::Return => Policy::Return,
                };
                Ok(OptionDirective::BlockPolicy(value))
            }
            "fail-policy" => Ok(OptionDirective::FailPolicy(self.parse_policy_value()?)),
            "require-order" => Ok(OptionDirective::RequireOrder(self.parse_yes_no()?)),
            "state-policy" => {
                if self.eat_word("floating") {
                    Ok(OptionDirective::StatePolicy(StatePolicyValue::Floating))
                } else if self.eat_word("if-bound") {
                    Ok(OptionDirective::StatePolicy(StatePolicyValue::IfBound))
                } else {
                    Err(self.err("expected `floating` or `if-bound`"))
                }
            }
            "skip" => {
                if !self.eat_word("on") {
                    return Err(self.err("expected `on` after `set skip`"));
                }
                Ok(OptionDirective::Skip(self.parse_iface_ref()?))
            }
            "optimization" => {
                let value = self.expect_word("`none`, `basic` or `profile`")?;
                if !matches!(value.as_str(), "none" | "basic" | "profile") {
                    return Err(self.err(format!("unknown optimization `{value}'")));
                }
                Ok(OptionDirective::Optimization(value))
            }
            "loginterface" => Ok(OptionDirective::LogInterface(
                self.expect_string("an interface name")?,
            )),
            "hostid" => {
                let value = self.expect_word("a hostid number")?;
                let value = value
                    .parse::<u32>()
                    .map_err(|_| self.err(format!("invalid hostid `{value}'")))?;
                if value == 0 {
                    return Err(self.err("hostid must be non-zero"));
                }
                Ok(OptionDirective::HostId(value))
            }
            "timeout" => Ok(OptionDirective::Timeout(
                self.parse_name_value_pairs("a timeout name")?,
            )),
            "limit" => Ok(OptionDirective::Limit(
                self.parse_name_value_pairs("a limit name")?,
            )),
            other => Err(self.err(format!("unsupported `set {other}' option"))),
        }
    }

    fn parse_policy_value(&mut self) -> Result<PolicyValue, ParseError> {
        if self.eat_word("drop") {
            Ok(PolicyValue::Drop)
        } else if self.eat_word("return") {
            Ok(PolicyValue::Return)
        } else {
            Err(self.err("expected `drop` or `return`"))
        }
    }

    fn parse_yes_no(&mut self) -> Result<bool, ParseError> {
        if self.eat_word("yes") {
            Ok(true)
        } else if self.eat_word("no") {
            Ok(false)
        } else {
            Err(self.err("expected `yes` or `no`"))
        }
    }

    fn parse_name_value_pairs(&mut self, what: &str) -> Result<Vec<(String, u32)>, ParseError> {
        let mut pairs = Vec::new();
        if self.eat_punct(&Token::LBrace) {
            loop {
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                pairs.push(self.parse_name_value_pair(what)?);
                self.skip_newlines();
                if self.eat_punct(&Token::Comma) {
                    continue;
                }
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                return Err(self.err("expected `,` or `}` in the option list"));
            }
        } else {
            pairs.push(self.parse_name_value_pair(what)?);
        }
        Ok(pairs)
    }

    fn parse_name_value_pair(&mut self, what: &str) -> Result<(String, u32), ParseError> {
        let name = self.expect_word(what)?;
        let value = self.expect_word("a number")?;
        let value = value
            .parse::<u32>()
            .map_err(|_| self.err(format!("invalid number `{value}' for `{name}'")))?;
        Ok((name, value))
    }

    fn parse_iface_ref(&mut self) -> Result<IfaceRef, ParseError> {
        if self.eat_punct(&Token::LBrace) {
            return Err(self.err("interface lists are not supported here"));
        }
        let negative = self.eat_punct(&Token::Bang);
        let name = self.expect_string("an interface name")?;
        Ok(IfaceRef { negative, name })
    }

    fn parse_filter_rule(&mut self, action: Action) -> Result<Rule, ParseError> {
        let mut rule = Rule {
            action,
            direction: None,
            log: false,
            quick: false,
            interface: None,
            af: None,
            proto: None,
            source: AddrPortSpec::default(),
            destination: AddrPortSpec::default(),
            flags: None,
            state: StateMode::Default,
        };
        if self.eat_word("in") {
            rule.direction = Some(Direction::In);
        } else if self.eat_word("out") {
            rule.direction = Some(Direction::Out);
        }
        loop {
            if self.eat_word("log") {
                rule.log = true;
            } else if self.eat_word("quick") {
                rule.quick = true;
            } else {
                break;
            }
        }
        if self.eat_word("on") {
            rule.interface = Some(self.parse_iface_ref()?);
        }
        if self.eat_word("inet") {
            rule.af = Some(AddressFamily::Inet);
        } else if self.eat_word("inet6") {
            rule.af = Some(AddressFamily::Inet6);
        }
        if self.eat_word("proto") {
            rule.proto = Some(self.parse_protocol()?);
        }
        self.parse_fromto(&mut rule.source, &mut rule.destination)?;
        let mut flags_seen = false;
        loop {
            if self.eat_word("flags") {
                if flags_seen {
                    return Err(self.err("multiple `flags` clauses"));
                }
                flags_seen = true;
                rule.flags = self.parse_flags()?;
            } else if self.eat_word("keep") {
                if !self.eat_word("state") {
                    return Err(self.err("expected `state` after `keep`"));
                }
                rule.state = StateMode::Keep;
            } else if self.eat_word("modulate") {
                if !self.eat_word("state") {
                    return Err(self.err("expected `state` after `modulate`"));
                }
                rule.state = StateMode::Modulate;
            } else if self.eat_word("synproxy") {
                if !self.eat_word("state") {
                    return Err(self.err("expected `state` after `synproxy`"));
                }
                rule.state = StateMode::Synproxy;
            } else if self.eat_word("no") {
                if !self.eat_word("state") {
                    return Err(self.err("expected `state` after `no`"));
                }
                rule.state = StateMode::None;
            } else {
                break;
            }
        }
        // FreeBSD keeps state by default on pass rules (parse.y `pfrule`).
        if rule.action == Action::Pass && rule.state == StateMode::Default {
            rule.state = StateMode::Keep;
        }
        Ok(rule)
    }

    fn parse_nat_rule(&mut self, kind: NatKind) -> Result<NatRule, ParseError> {
        let mut rule = NatRule {
            kind,
            pass: false,
            log: false,
            quick: false,
            interface: None,
            af: None,
            proto: None,
            source: AddrPortSpec::default(),
            destination: AddrPortSpec::default(),
            redirect: None,
        };
        if matches!(kind, NatKind::Nat | NatKind::Rdr | NatKind::Binat) && self.eat_word("pass") {
            rule.pass = true;
        }
        loop {
            if self.eat_word("log") {
                rule.log = true;
            } else if self.eat_word("quick") {
                rule.quick = true;
            } else {
                break;
            }
        }
        if self.eat_word("on") {
            rule.interface = Some(self.parse_iface_ref()?);
        }
        if self.eat_word("inet") {
            rule.af = Some(AddressFamily::Inet);
        } else if self.eat_word("inet6") {
            rule.af = Some(AddressFamily::Inet6);
        }
        if self.eat_word("proto") {
            rule.proto = Some(self.parse_protocol()?);
        }
        self.parse_fromto(&mut rule.source, &mut rule.destination)?;
        let negated = matches!(kind, NatKind::NoNat | NatKind::NoRdr | NatKind::NoBinat);
        if self.eat_punct(&Token::Arrow) {
            if negated {
                return Err(self.err("a `no` translation rule does not take `->'"));
            }
            rule.redirect = Some(self.parse_redirect()?);
        } else if !negated {
            return Err(self.err("a translation rule requires `-> address'"));
        }
        Ok(rule)
    }

    fn parse_redirect(&mut self) -> Result<Redirect, ParseError> {
        let address = if self.eat_punct(&Token::Lt) {
            let name = self.expect_word("a table name")?;
            self.expect_punct(&Token::Gt, "`>`")?;
            HostAddr::Table(name)
        } else {
            let word = self.expect_word("a translation address")?;
            if !is_address_literal(&word) {
                return Err(self.err(format!("invalid translation address `{word}'")));
            }
            HostAddr::Literal(word)
        };
        let mut port = None;
        if self.eat_word("port") {
            let word = self.expect_word("a port number")?;
            port = Some(
                parse_port_number(&word)
                    .ok_or_else(|| self.err(format!("invalid translated port `{word}'")))?,
            );
        }
        let pool = if self.eat_word("round-robin") {
            Some(PoolOption::RoundRobin)
        } else if self.eat_word("source-hash") {
            Some(PoolOption::SourceHash)
        } else if self.eat_word("bitmask") {
            Some(PoolOption::Bitmask)
        } else {
            None
        };
        Ok(Redirect {
            address,
            port,
            pool,
        })
    }

    fn parse_protocol(&mut self) -> Result<Protocol, ParseError> {
        let word = self.expect_word("a protocol")?;
        match word.as_str() {
            "tcp" => Ok(Protocol::Tcp),
            "udp" => Ok(Protocol::Udp),
            "icmp" => Ok(Protocol::Icmp),
            "icmp6" => Ok(Protocol::Icmp6),
            other => other
                .parse::<u8>()
                .map(Protocol::Number)
                .map_err(|_| self.err(format!("unsupported protocol `{other}'"))),
        }
    }

    fn parse_fromto(
        &mut self,
        source: &mut AddrPortSpec,
        destination: &mut AddrPortSpec,
    ) -> Result<(), ParseError> {
        if self.eat_word("all") {
            return Ok(());
        }
        if self.eat_word("from") {
            *source = self.parse_addrspec()?;
        }
        if self.eat_word("to") {
            *destination = self.parse_addrspec()?;
        }
        Ok(())
    }

    fn parse_addrspec(&mut self) -> Result<AddrPortSpec, ParseError> {
        let mut spec = AddrPortSpec::default();
        if self.eat_word("any") {
            if self.eat_word("port") {
                let port = self.parse_portspec()?;
                spec.items.push(AddrItem {
                    negative: false,
                    address: HostAddr::Any,
                    port: Some(port),
                });
            }
            return Ok(spec);
        }
        if matches!(self.peek_token(), Token::Word(word) if word == "port") {
            let port = self.parse_portspec()?;
            spec.items.push(AddrItem {
                negative: false,
                address: HostAddr::Any,
                port: Some(port),
            });
            return Ok(spec);
        }
        if self.eat_punct(&Token::LBrace) {
            loop {
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                spec.items.push(self.parse_addr_item()?);
                self.skip_newlines();
                if self.eat_punct(&Token::Comma) {
                    continue;
                }
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                return Err(self.err("expected `,` or `}` in the address list"));
            }
            if self.eat_word("port") {
                let port = self.parse_portspec()?;
                for item in &mut spec.items {
                    item.port = Some(port.clone());
                }
            }
            return Ok(spec);
        }
        let item = self.parse_addr_item()?;
        spec.items.push(item);
        Ok(spec)
    }

    fn parse_addr_item(&mut self) -> Result<AddrItem, ParseError> {
        let negative = self.eat_punct(&Token::Bang);
        if self.eat_word("any") {
            return Ok(AddrItem {
                negative,
                address: HostAddr::Any,
                port: None,
            });
        }
        let address = if self.eat_punct(&Token::Lt) {
            let name = self.expect_word("a table name")?;
            self.expect_punct(&Token::Gt, "`>`")?;
            if negative {
                // A table in a `!` position is valid; `<` already consumed.
            }
            HostAddr::Table(name)
        } else {
            let word = self.expect_word("an address, `<table>` or `any`")?;
            if word == "no-route" || word == "urpf-failed" {
                return Err(self.err(format!("`{word}' addresses are not supported")));
            }
            if word.contains('-') && !word.contains('/') {
                return Err(self.err(format!("address ranges are not supported (`{word}')")));
            }
            if !is_address_literal(&word) {
                return Err(self.err(format!(
                    "invalid address `{word}' (hostnames are not resolved)"
                )));
            }
            HostAddr::Literal(word)
        };
        let port = if self.eat_word("port") {
            Some(self.parse_portspec()?)
        } else {
            None
        };
        Ok(AddrItem {
            negative,
            address,
            port,
        })
    }

    fn parse_portspec(&mut self) -> Result<PortSpec, ParseError> {
        if self.eat_punct(&Token::LBrace) {
            let mut items = Vec::new();
            loop {
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                items.push(self.parse_port_item()?);
                self.skip_newlines();
                if self.eat_punct(&Token::Comma) {
                    continue;
                }
                self.skip_newlines();
                if self.eat_punct(&Token::RBrace) {
                    break;
                }
                return Err(self.err("expected `,` or `}` in the port list"));
            }
            if items.is_empty() {
                return Err(self.err("empty port list"));
            }
            return Ok(PortSpec::Set(items));
        }
        self.parse_port_item()
    }

    fn parse_port_item(&mut self) -> Result<PortSpec, ParseError> {
        let operator = match self.peek_token() {
            Token::Eq => {
                self.next();
                Some(Token::Eq)
            }
            Token::Ne => {
                self.next();
                Some(Token::Ne)
            }
            Token::Le => {
                self.next();
                Some(Token::Le)
            }
            Token::Ge => {
                self.next();
                Some(Token::Ge)
            }
            Token::Lt => {
                self.next();
                Some(Token::Lt)
            }
            Token::Gt => {
                self.next();
                Some(Token::Gt)
            }
            _ => None,
        };
        let word = self.expect_word("a port number")?;
        if let Some((low, high)) = word.split_once(':') {
            if operator.is_some() {
                return Err(self.err("a port range does not take a comparison operator"));
            }
            let low =
                parse_port_number(low).ok_or_else(|| self.err(format!("invalid port `{low}'")))?;
            let high = parse_port_number(high)
                .ok_or_else(|| self.err(format!("invalid port `{high}'")))?;
            if low > high {
                return Err(self.err(format!("port range `{word}' is inverted")));
            }
            return Ok(PortSpec::Range(low, high));
        }
        let port =
            parse_port_number(&word).ok_or_else(|| self.err(format!("invalid port `{word}'")))?;
        Ok(match operator {
            None | Some(Token::Eq) => PortSpec::Eq(port),
            Some(Token::Ne) => PortSpec::Ne(port),
            Some(Token::Lt) => PortSpec::Lt(port),
            Some(Token::Le) => PortSpec::Le(port),
            Some(Token::Gt) => PortSpec::Gt(port),
            Some(Token::Ge) => PortSpec::Ge(port),
            Some(_) => unreachable!("operator selection is exhaustive"),
        })
    }

    fn parse_flags(&mut self) -> Result<Option<FlagsSpec>, ParseError> {
        let word = self.expect_word("`set/mask' or `any'")?;
        if word == "any" {
            // parse.y `FLAGS ANY`: flags and flagset are both zero, i.e. the
            // implicit form (the printer then emits `flags any` when applicable).
            return Ok(None);
        }
        let Some((set, mask)) = word.split_once('/') else {
            return Err(self.err("flags must be written as `set/mask'"));
        };
        if mask.is_empty() {
            return Err(self.err("flags must be written as `set/mask'"));
        }
        for letter in set.chars().chain(mask.chars()) {
            if !"FSRPAUEC".contains(letter) {
                return Err(self.err(format!("invalid TCP flag `{letter}'")));
            }
        }
        if !set.chars().all(|letter| mask.contains(letter)) {
            return Err(self.err("the flags set must be a subset of the mask"));
        }
        Ok(Some(FlagsSpec {
            set: String::from(set),
            mask: String::from(mask),
        }))
    }
}

fn parse_port_number(text: &str) -> Option<u16> {
    let value = text.parse::<u16>().ok()?;
    (value >= 1).then_some(value)
}

#[cfg(test)]
#[path = "parser_ut.rs"]
mod tests;
