// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX awk core: records/fields, rules, expressions, arrays and built-ins.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_libc_regex::consts::{REG_BASIC, REG_NOMATCH};
use raven_libc_regex::locale::ByteLocale;
use raven_libc_regex::{RegMatch, Regex, regcomp, regexec};

/// Why a script or input was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwkError {
    /// Unsupported or malformed script.
    Usage,
    /// Runtime error.
    Runtime,
}

/// Options applied before BEGIN.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    /// Field separator (`-F`).
    pub fs: Option<String>,
    /// `-v name=value` assignments.
    pub vars: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Num(f64),
    Str(String),
    Regex(String),
    Ident(String),
    Op(&'static str),
    Newline,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Num(f64),
    Str(String),
    Regex(String),
    Var(String),
    Field(Box<Expr>),
    Index(String, Box<Expr>),
    Unary(&'static str, Box<Expr>),
    Inc(Box<Expr>, i32, bool),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
enum Stmt {
    Expr(Expr),
    Print(Vec<Expr>),
    Printf(Vec<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    For(Option<Expr>, Option<Expr>, Option<Expr>, Box<Stmt>),
    ForIn(String, String, Box<Stmt>),
    Block(Vec<Stmt>),
    Break,
    Continue,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
enum Rule {
    Begin(Stmt),
    End(Stmt),
    Action(Option<Expr>, Option<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
struct Program {
    rules: Vec<Rule>,
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Num(f64),
    Str(String),
    Array(BTreeMap<String, Value>),
}

struct Lexer {
    chars: Vec<char>,
    position: usize,
    can_end: bool,
}

impl Lexer {
    fn new(script: &str) -> Self {
        Self {
            chars: script.chars().collect(),
            position: 0,
            can_end: false,
        }
    }

    fn tokens(mut self) -> Result<Vec<Token>, AwkError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let end = token == Token::Eof;
            self.can_end = matches!(
                token,
                Token::Num(_)
                    | Token::Str(_)
                    | Token::Regex(_)
                    | Token::Ident(_)
                    | Token::Op(")")
                    | Token::Op("]")
            );
            tokens.push(token);
            if end {
                return Ok(tokens);
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, AwkError> {
        loop {
            let Some(character) = self.chars.get(self.position).copied() else {
                return Ok(Token::Eof);
            };
            if character == ' ' || character == '\t' || character == '\r' {
                self.position = self.position.saturating_add(1);
                continue;
            }
            if character == '#' {
                while let Some(value) = self.chars.get(self.position).copied() {
                    self.position = self.position.saturating_add(1);
                    if value == '\n' {
                        return Ok(Token::Newline);
                    }
                }
                return Ok(Token::Eof);
            }
            if character == '\n' {
                self.position = self.position.saturating_add(1);
                return Ok(Token::Newline);
            }
            if character.is_ascii_digit()
                || (character == '.'
                    && self
                        .chars
                        .get(self.position + 1)
                        .is_some_and(|c| c.is_ascii_digit()))
            {
                return self.number();
            }
            if character == '"' {
                return self.string();
            }
            if character.is_ascii_alphabetic() || character == '_' {
                return self.ident();
            }
            if character == '/' && !self.can_end {
                return self.regex();
            }
            let rest: String = self.chars.iter().skip(self.position).take(3).collect();
            for operator in [
                "++", "--", "+=", "-=", "*=", "/=", "%=", "^=", "==", "!=", "<=", ">=", "&&", "||",
                "!~", "~", "!", "+", "-", "*", "/", "%", "^", "=", "<", ">", "(", ")", "{", "}",
                "[", "]", ",", ";", "$", "?", ":",
            ] {
                if rest.starts_with(operator) {
                    self.position = self.position.saturating_add(operator.chars().count());
                    return Ok(Token::Op(operator));
                }
            }
            let _ = character;
            return Err(AwkError::Usage);
        }
    }

    fn number(&mut self) -> Result<Token, AwkError> {
        let start = self.position;
        let mut seen_dot = false;
        while let Some(character) = self.chars.get(self.position).copied() {
            if character.is_ascii_digit() {
                self.position = self.position.saturating_add(1);
            } else if character == '.' && !seen_dot {
                seen_dot = true;
                self.position = self.position.saturating_add(1);
            } else {
                break;
            }
        }
        let text: String = self
            .chars
            .get(start..self.position)
            .unwrap_or(&[])
            .iter()
            .collect();
        text.parse::<f64>()
            .map(Token::Num)
            .map_err(|_| AwkError::Usage)
    }

    fn string(&mut self) -> Result<Token, AwkError> {
        self.position = self.position.saturating_add(1);
        let mut value = String::new();
        while let Some(character) = self.chars.get(self.position).copied() {
            self.position = self.position.saturating_add(1);
            if character == '"' {
                return Ok(Token::Str(value));
            }
            if character == '\\' {
                let Some(next) = self.chars.get(self.position).copied() else {
                    return Err(AwkError::Usage);
                };
                self.position = self.position.saturating_add(1);
                match next {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    other => value.push(other),
                }
            } else {
                value.push(character);
            }
        }
        Err(AwkError::Usage)
    }

    fn ident(&mut self) -> Result<Token, AwkError> {
        let start = self.position;
        while let Some(character) = self.chars.get(self.position).copied() {
            if character.is_ascii_alphanumeric() || character == '_' {
                self.position = self.position.saturating_add(1);
            } else {
                break;
            }
        }
        Ok(Token::Ident(
            self.chars
                .get(start..self.position)
                .unwrap_or(&[])
                .iter()
                .collect(),
        ))
    }

    fn regex(&mut self) -> Result<Token, AwkError> {
        self.position = self.position.saturating_add(1);
        let mut value = String::new();
        while let Some(character) = self.chars.get(self.position).copied() {
            self.position = self.position.saturating_add(1);
            if character == '/' {
                return Ok(Token::Regex(value));
            }
            if character == '\\' {
                value.push(character);
                if let Some(next) = self.chars.get(self.position).copied() {
                    self.position = self.position.saturating_add(1);
                    value.push(next);
                }
            } else {
                value.push(character);
            }
        }
        Err(AwkError::Usage)
    }
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::Eof)
    }

    fn peek_op(&self, operator: &str) -> bool {
        matches!(self.peek(), Token::Op(value) if *value == operator)
    }

    fn peek_ident(&self, value: &str) -> bool {
        self.peek() == &Token::Ident(value.to_string())
    }

    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        self.position = self.position.saturating_add(1);
        token
    }

    fn eat_op(&mut self, operator: &str) -> bool {
        if self.peek_op(operator) {
            self.position = self.position.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Token::Newline) || self.peek_op(";") {
            self.position = self.position.saturating_add(1);
        }
    }

    fn parse_program(mut self) -> Result<Program, AwkError> {
        let mut rules = Vec::new();
        self.skip_separators();
        while !matches!(self.peek(), Token::Eof) {
            if self.peek_ident("function") {
                return Err(AwkError::Usage);
            }
            if self.peek_ident("BEGIN") {
                self.next();
                rules.push(Rule::Begin(self.parse_block()?));
            } else if self.peek_ident("END") {
                self.next();
                rules.push(Rule::End(self.parse_block()?));
            } else {
                let pattern = if self.peek_op("{") {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                let action = if self.peek_op("{") {
                    Some(self.parse_block()?)
                } else {
                    None
                };
                rules.push(Rule::Action(pattern, action));
            }
            self.skip_separators();
        }
        Ok(Program { rules })
    }

    fn parse_block(&mut self) -> Result<Stmt, AwkError> {
        if !self.eat_op("{") {
            return Err(AwkError::Usage);
        }
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.peek_op("}") {
            if matches!(self.peek(), Token::Eof) {
                return Err(AwkError::Usage);
            }
            statements.push(self.parse_stmt()?);
            self.skip_separators();
        }
        self.next();
        Ok(Stmt::Block(statements))
    }

    fn parse_stmt(&mut self) -> Result<Stmt, AwkError> {
        if self.peek_op("{") {
            return self.parse_block();
        }
        if self.peek_ident("if") {
            self.next();
            return self.parse_if();
        }
        if self.peek_ident("while") {
            self.next();
            self.expect_op("(")?;
            let condition = self.parse_expr()?;
            self.expect_op(")")?;
            let body = Box::new(self.parse_stmt()?);
            return Ok(Stmt::While(condition, body));
        }
        if self.peek_ident("for") {
            self.next();
            return self.parse_for();
        }
        if self.peek_ident("print") {
            self.next();
            return Ok(Stmt::Print(self.parse_print_list()?));
        }
        if self.peek_ident("printf") {
            self.next();
            return Ok(Stmt::Printf(self.parse_print_list()?));
        }
        if self.peek_ident("break") {
            self.next();
            return Ok(Stmt::Break);
        }
        if self.peek_ident("continue") {
            self.next();
            return Ok(Stmt::Continue);
        }
        if self.peek_ident("next") {
            self.next();
            return Ok(Stmt::Next);
        }
        Ok(Stmt::Expr(self.parse_expr()?))
    }

    fn parse_if(&mut self) -> Result<Stmt, AwkError> {
        self.expect_op("(")?;
        let condition = self.parse_expr()?;
        self.expect_op(")")?;
        let then = Box::new(self.parse_stmt()?);
        self.skip_separators();
        let otherwise = if self.peek_ident("else") {
            self.next();
            Some(Box::new(self.parse_stmt()?))
        } else {
            None
        };
        Ok(Stmt::If(condition, then, otherwise))
    }

    fn parse_for(&mut self) -> Result<Stmt, AwkError> {
        self.expect_op("(")?;
        if let Token::Ident(variable) = self.peek().clone()
            && self.tokens.get(self.position + 1) == Some(&Token::Ident("in".to_string()))
        {
            self.next();
            self.next();
            let array = match self.next() {
                Token::Ident(name) => name,
                _ => return Err(AwkError::Usage),
            };
            self.expect_op(")")?;
            let body = Box::new(self.parse_stmt()?);
            return Ok(Stmt::ForIn(variable, array, body));
        }
        let init = if self.peek_op(";") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_op(";")?;
        let condition = if self.peek_op(";") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_op(";")?;
        let increment = if self.peek_op(")") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_op(")")?;
        let body = Box::new(self.parse_stmt()?);
        Ok(Stmt::For(init, condition, increment, body))
    }

    fn parse_print_list(&mut self) -> Result<Vec<Expr>, AwkError> {
        let mut values = Vec::new();
        if matches!(self.peek(), Token::Newline | Token::Eof)
            || self.peek_op(";")
            || self.peek_op("}")
        {
            return Ok(values);
        }
        values.push(self.parse_expr()?);
        while self.eat_op(",") {
            values.push(self.parse_expr()?);
        }
        Ok(values)
    }

    fn expect_op(&mut self, operator: &str) -> Result<(), AwkError> {
        if self.eat_op(operator) {
            Ok(())
        } else {
            Err(AwkError::Usage)
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, AwkError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, AwkError> {
        let left = self.parse_or()?;
        for operator in ["=", "+=", "-=", "*=", "/=", "%=", "^="] {
            if self.peek_op(operator) {
                self.next();
                let right = self.parse_assignment()?;
                if !is_lvalue(&left) {
                    return Err(AwkError::Usage);
                }
                return Ok(Expr::Assign(operator, Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_and()?;
        while self.eat_op("||") {
            let right = self.parse_and()?;
            left = Expr::Binary("||", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_in()?;
        while self.eat_op("&&") {
            let right = self.parse_in()?;
            left = Expr::Binary("&&", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_in(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_match()?;
        if self.peek_ident("in") {
            self.next();
            let array = match self.next() {
                Token::Ident(name) => name,
                _ => return Err(AwkError::Usage),
            };
            left = Expr::Binary("in", Box::new(left), Box::new(Expr::Var(array)));
        }
        Ok(left)
    }

    fn parse_match(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_compare()?;
        loop {
            if self.peek_op("~") || self.peek_op("!~") {
                let operator = match self.next() {
                    Token::Op(value) => value,
                    _ => return Err(AwkError::Usage),
                };
                let right = self.parse_compare()?;
                left = Expr::Binary(operator, Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_compare(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_concat()?;
        loop {
            let operator = match self.peek() {
                Token::Op(value @ ("==" | "!=" | "<" | "<=" | ">" | ">=")) => Some(*value),
                _ => None,
            };
            if let Some(operator) = operator {
                self.next();
                let right = self.parse_concat()?;
                left = Expr::Binary(operator, Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_concat(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_add()?;
        while can_start_primary(self.peek()) {
            let right = self.parse_add()?;
            left = Expr::Binary("concat", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_mul()?;
        loop {
            let operator = match self.peek() {
                Token::Op(value @ ("+" | "-")) => Some(*value),
                _ => None,
            };
            if let Some(operator) = operator {
                self.next();
                let right = self.parse_mul()?;
                left = Expr::Binary(operator, Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_mul(&mut self) -> Result<Expr, AwkError> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = match self.peek() {
                Token::Op(value @ ("*" | "/" | "%")) => Some(*value),
                _ => None,
            };
            if let Some(operator) = operator {
                self.next();
                let right = self.parse_unary()?;
                left = Expr::Binary(operator, Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, AwkError> {
        if self.peek_op("!") || self.peek_op("-") || self.peek_op("+") {
            let operator = match self.next() {
                Token::Op(value) => value,
                _ => return Err(AwkError::Usage),
            };
            let value = self.parse_unary()?;
            return Ok(Expr::Unary(operator, Box::new(value)));
        }
        if self.peek_op("++") || self.peek_op("--") {
            let operator = match self.next() {
                Token::Op(value) => value,
                _ => return Err(AwkError::Usage),
            };
            let value = self.parse_unary()?;
            if !is_lvalue(&value) {
                return Err(AwkError::Usage);
            }
            return Ok(Expr::Inc(
                Box::new(value),
                if operator == "++" { 1 } else { -1 },
                false,
            ));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<Expr, AwkError> {
        let left = self.parse_postfix()?;
        if self.eat_op("^") {
            let right = self.parse_unary()?;
            return Ok(Expr::Binary("^", Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn parse_postfix(&mut self) -> Result<Expr, AwkError> {
        let value = self.parse_primary()?;
        if self.peek_op("++") || self.peek_op("--") {
            let operator = match self.next() {
                Token::Op(value) => value,
                _ => return Err(AwkError::Usage),
            };
            if !is_lvalue(&value) {
                return Err(AwkError::Usage);
            }
            return Ok(Expr::Inc(
                Box::new(value),
                if operator == "++" { 1 } else { -1 },
                true,
            ));
        }
        Ok(value)
    }

    fn parse_primary(&mut self) -> Result<Expr, AwkError> {
        match self.next() {
            Token::Num(value) => Ok(Expr::Num(value)),
            Token::Str(value) => Ok(Expr::Str(value)),
            Token::Regex(value) => Ok(Expr::Regex(value)),
            Token::Ident(name) => {
                if matches!(name.as_str(), "getline" | "close" | "system") {
                    return Err(AwkError::Usage);
                }
                if self.peek_op("(") {
                    self.next();
                    let mut arguments = Vec::new();
                    if !self.peek_op(")") {
                        arguments.push(self.parse_expr()?);
                        while self.eat_op(",") {
                            arguments.push(self.parse_expr()?);
                        }
                    }
                    self.expect_op(")")?;
                    Ok(Expr::Call(name, arguments))
                } else if self.peek_op("[") {
                    self.next();
                    let index = self.parse_expr()?;
                    self.expect_op("]")?;
                    Ok(Expr::Index(name, Box::new(index)))
                } else if name == "length" {
                    Ok(Expr::Call(name, Vec::new()))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Token::Op("$") => {
                let index = self.parse_primary()?;
                Ok(Expr::Field(Box::new(index)))
            }
            Token::Op("(") => {
                let value = self.parse_expr()?;
                self.expect_op(")")?;
                Ok(value)
            }
            _ => Err(AwkError::Usage),
        }
    }
}

fn is_lvalue(expr: &Expr) -> bool {
    matches!(expr, Expr::Var(_) | Expr::Field(_) | Expr::Index(_, _))
}

fn can_start_primary(token: &Token) -> bool {
    match token {
        Token::Num(_) | Token::Str(_) | Token::Regex(_) => true,
        Token::Ident(name) => !matches!(
            name.as_str(),
            "in" | "else"
                | "if"
                | "while"
                | "for"
                | "print"
                | "printf"
                | "break"
                | "continue"
                | "next"
                | "BEGIN"
                | "END"
                | "function"
                | "getline"
                | "close"
        ),
        Token::Op("(") | Token::Op("$") => true,
        _ => false,
    }
}

/// Run one bounded awk program over `input`.
pub fn run(script: &str, input: &str, options: &Options) -> Result<String, AwkError> {
    let program = Parser::new(Lexer::new(script).tokens()?).parse_program()?;
    let mut runtime = Runtime::new(options);
    runtime.execute(&program, input)
}

struct Runtime {
    vars: BTreeMap<String, Value>,
    record: String,
    fields: Vec<String>,
    output: String,
    next: bool,
}

impl Runtime {
    fn new(options: &Options) -> Self {
        let mut vars = BTreeMap::new();
        vars.insert(
            "FS".to_string(),
            Value::Str(options.fs.clone().unwrap_or_else(|| " ".to_string())),
        );
        vars.insert("OFS".to_string(), Value::Str(" ".to_string()));
        vars.insert("ORS".to_string(), Value::Str("\n".to_string()));
        vars.insert("NR".to_string(), Value::Num(0.0));
        vars.insert("NF".to_string(), Value::Num(0.0));
        vars.insert("RSTART".to_string(), Value::Num(0.0));
        vars.insert("RLENGTH".to_string(), Value::Num(-1.0));
        for (name, value) in &options.vars {
            vars.insert(name.clone(), Value::Str(value.clone()));
        }
        Self {
            vars,
            record: String::new(),
            fields: Vec::new(),
            output: String::new(),
            next: false,
        }
    }

    fn execute(&mut self, program: &Program, input: &str) -> Result<String, AwkError> {
        let begin: Vec<&Stmt> = program
            .rules
            .iter()
            .filter_map(|rule| match rule {
                Rule::Begin(stmt) => Some(stmt),
                _ => None,
            })
            .collect();
        for stmt in begin {
            self.exec(stmt, 0)?;
        }
        let records: Vec<&str> = if input.is_empty() {
            Vec::new()
        } else {
            input.lines().collect()
        };
        for (index, line) in records.iter().enumerate() {
            self.set_record(line);
            self.set_var("NR", Value::Num((index + 1) as f64));
            self.next = false;
            for rule in &program.rules {
                if let Rule::Action(pattern, action) = rule {
                    let selected = match pattern {
                        Some(expression) => self.eval_truthy(expression)?,
                        None => true,
                    };
                    if selected {
                        if let Some(action) = action {
                            if let Control::Next = self.exec(action, 0)? {
                                self.next = true;
                            }
                        } else {
                            self.print_values(&[]);
                        }
                        if self.next {
                            break;
                        }
                    }
                }
            }
        }
        let end: Vec<&Stmt> = program
            .rules
            .iter()
            .filter_map(|rule| match rule {
                Rule::End(stmt) => Some(stmt),
                _ => None,
            })
            .collect();
        for stmt in end {
            self.exec(stmt, 0)?;
        }
        Ok(core::mem::take(&mut self.output))
    }

    fn exec(&mut self, stmt: &Stmt, depth: usize) -> Result<Control, AwkError> {
        if depth > 64 {
            return Err(AwkError::Runtime);
        }
        match stmt {
            Stmt::Expr(expression) => {
                let _ = self.eval(expression)?;
                Ok(Control::None)
            }
            Stmt::Print(values) => {
                self.print_values(values);
                Ok(Control::None)
            }
            Stmt::Printf(values) => {
                self.printf_values(values);
                Ok(Control::None)
            }
            Stmt::If(condition, then, otherwise) => {
                if self.eval_truthy(condition)? {
                    self.exec(then, depth + 1)
                } else if let Some(otherwise) = otherwise {
                    self.exec(otherwise, depth + 1)
                } else {
                    Ok(Control::None)
                }
            }
            Stmt::While(condition, body) => {
                let mut guard = 0usize;
                while self.eval_truthy(condition)? {
                    guard = guard.saturating_add(1);
                    if guard > 1_000_000 {
                        return Err(AwkError::Runtime);
                    }
                    match self.exec(body, depth + 1)? {
                        Control::Break => break,
                        Control::Next => return Ok(Control::Next),
                        _ => {}
                    }
                }
                Ok(Control::None)
            }
            Stmt::For(init, condition, increment, body) => {
                if let Some(init) = init {
                    let _ = self.eval(init)?;
                }
                let mut guard = 0usize;
                loop {
                    if let Some(condition) = condition
                        && !self.eval_truthy(condition)?
                    {
                        break;
                    }
                    guard = guard.saturating_add(1);
                    if guard > 1_000_000 {
                        return Err(AwkError::Runtime);
                    }
                    match self.exec(body, depth + 1)? {
                        Control::Break => break,
                        Control::Next => return Ok(Control::Next),
                        _ => {}
                    }
                    if let Some(increment) = increment {
                        let _ = self.eval(increment)?;
                    }
                }
                Ok(Control::None)
            }
            Stmt::ForIn(variable, array, body) => {
                let keys = match self.vars.get(array) {
                    Some(Value::Array(values)) => values.keys().cloned().collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                for key in keys {
                    self.vars.insert(variable.clone(), Value::Str(key));
                    match self.exec(body, depth + 1)? {
                        Control::Break => break,
                        Control::Next => return Ok(Control::Next),
                        _ => {}
                    }
                }
                Ok(Control::None)
            }
            Stmt::Block(statements) => {
                for statement in statements {
                    match self.exec(statement, depth + 1)? {
                        Control::None => {}
                        control => return Ok(control),
                    }
                }
                Ok(Control::None)
            }
            Stmt::Break => Ok(Control::Break),
            Stmt::Continue => Ok(Control::Continue),
            Stmt::Next => Ok(Control::Next),
        }
    }

    fn eval_number(&mut self, expression: &Expr) -> Result<f64, AwkError> {
        let value = self.eval(expression)?;
        Ok(self.to_number(&value))
    }

    fn eval_string(&mut self, expression: &Expr) -> Result<String, AwkError> {
        let value = self.eval(expression)?;
        Ok(self.to_string(&value))
    }

    fn eval_truthy(&mut self, expression: &Expr) -> Result<bool, AwkError> {
        let value = self.eval(expression)?;
        Ok(self.truthy(&value))
    }

    fn get_string(&mut self, name: &str) -> String {
        let value = self.get_var(name);
        self.to_string(&value)
    }

    fn eval(&mut self, expression: &Expr) -> Result<Value, AwkError> {
        match expression {
            Expr::Num(value) => Ok(Value::Num(*value)),
            Expr::Str(value) => Ok(Value::Str(value.clone())),
            Expr::Regex(pattern) => {
                let regex = compile(pattern)?;
                Ok(Value::Num(
                    if regex_matches(&regex, self.record.as_bytes())? {
                        1.0
                    } else {
                        0.0
                    },
                ))
            }
            Expr::Var(name) => Ok(self.get_var(name)),
            Expr::Field(index) => {
                let index = self.eval_number(index)? as usize;
                Ok(Value::Str(self.get_field(index)))
            }
            Expr::Index(name, index) => {
                let key = self.eval_string(index)?;
                Ok(self
                    .get_array(name)
                    .get(&key)
                    .cloned()
                    .unwrap_or(Value::Str(String::new())))
            }
            Expr::Unary(operator, value) => {
                let value = self.eval(value)?;
                match *operator {
                    "!" => Ok(Value::Num(if self.truthy(&value) { 0.0 } else { 1.0 })),
                    "-" => Ok(Value::Num(-self.to_number(&value))),
                    _ => Ok(Value::Num(self.to_number(&value))),
                }
            }
            Expr::Inc(value, amount, post) => {
                let current = self.read_lvalue(value)?;
                let old = self.to_number(&current);
                let new = old + f64::from(*amount);
                self.assign(value, Value::Num(new))?;
                Ok(Value::Num(if *post { old } else { new }))
            }
            Expr::Binary(operator, left, right) => self.binary(operator, left, right),
            Expr::Assign(operator, left, right) => {
                let right = self.eval(right)?;
                let value = if *operator == "=" {
                    right
                } else {
                    let left_value = self.read_lvalue(left)?;
                    let current = self.to_number(&left_value);
                    let right_number = self.to_number(&right);
                    let result = match *operator {
                        "+=" => current + right_number,
                        "-=" => current - right_number,
                        "*=" => current * right_number,
                        "/=" => current / right_number,
                        "%=" => current % right_number,
                        "^=" => power(current, right_number),
                        _ => return Err(AwkError::Usage),
                    };
                    Value::Num(result)
                };
                self.assign(left, value.clone())?;
                Ok(value)
            }
            Expr::Call(name, arguments) => self.call(name, arguments),
        }
    }

    fn binary(&mut self, operator: &str, left: &Expr, right: &Expr) -> Result<Value, AwkError> {
        if operator == "&&" {
            let left = self.eval(left)?;
            if !self.truthy(&left) {
                return Ok(Value::Num(0.0));
            }
            let right = self.eval(right)?;
            return Ok(Value::Num(if self.truthy(&right) { 1.0 } else { 0.0 }));
        }
        if operator == "||" {
            let left = self.eval(left)?;
            if self.truthy(&left) {
                return Ok(Value::Num(1.0));
            }
            let right = self.eval(right)?;
            return Ok(Value::Num(if self.truthy(&right) { 1.0 } else { 0.0 }));
        }
        if operator == "in" {
            let key = self.eval_string(left)?;
            let Expr::Var(array) = right else {
                return Err(AwkError::Usage);
            };
            let found = self
                .vars
                .get(array)
                .and_then(|value| match value {
                    Value::Array(values) => Some(values.contains_key(&key)),
                    _ => None,
                })
                .unwrap_or(false);
            return Ok(Value::Num(if found { 1.0 } else { 0.0 }));
        }
        if operator == "~" || operator == "!~" {
            let value = self.eval_string(left)?;
            let pattern = self.pattern(right)?;
            let regex = compile(&pattern)?;
            let matched = regex_matches(&regex, value.as_bytes())?;
            return Ok(Value::Num(if matched == (operator == "~") {
                1.0
            } else {
                0.0
            }));
        }
        let left = self.eval(left)?;
        let right = self.eval(right)?;
        match operator {
            "concat" => Ok(Value::Str(format!(
                "{}{}",
                self.to_string(&left),
                self.to_string(&right)
            ))),
            "+" => Ok(Value::Num(self.to_number(&left) + self.to_number(&right))),
            "-" => Ok(Value::Num(self.to_number(&left) - self.to_number(&right))),
            "*" => Ok(Value::Num(self.to_number(&left) * self.to_number(&right))),
            "/" => Ok(Value::Num(self.to_number(&left) / self.to_number(&right))),
            "%" => Ok(Value::Num(self.to_number(&left) % self.to_number(&right))),
            "^" => Ok(Value::Num(power(
                self.to_number(&left),
                self.to_number(&right),
            ))),
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                let order = self.compare(&left, &right);
                let result = match operator {
                    "==" => order == core::cmp::Ordering::Equal,
                    "!=" => order != core::cmp::Ordering::Equal,
                    "<" => order == core::cmp::Ordering::Less,
                    "<=" => order != core::cmp::Ordering::Greater,
                    ">" => order == core::cmp::Ordering::Greater,
                    _ => order != core::cmp::Ordering::Less,
                };
                Ok(Value::Num(if result { 1.0 } else { 0.0 }))
            }
            _ => Err(AwkError::Usage),
        }
    }

    fn call(&mut self, name: &str, arguments: &[Expr]) -> Result<Value, AwkError> {
        match name {
            "length" => {
                let value = if let Some(argument) = arguments.first() {
                    self.eval_string(argument)?
                } else {
                    self.record.clone()
                };
                Ok(Value::Num(value.chars().count() as f64))
            }
            "substr" => {
                if arguments.len() < 2 {
                    return Err(AwkError::Usage);
                }
                let value = self.eval_string(&arguments[0])?;
                let start = (self.eval_number(&arguments[1])? as isize).max(1) as usize;
                let length = if let Some(length) = arguments.get(2) {
                    self.eval_number(length)? as usize
                } else {
                    usize::MAX
                };
                let text: Vec<char> = value
                    .chars()
                    .skip(start.saturating_sub(1))
                    .take(length)
                    .collect();
                Ok(Value::Str(text.into_iter().collect()))
            }
            "index" => {
                if arguments.len() != 2 {
                    return Err(AwkError::Usage);
                }
                let value = self.eval_string(&arguments[0])?;
                let needle = self.eval_string(&arguments[1])?;
                let position = value
                    .find(&needle)
                    .map_or(0, |index| value[..index].chars().count() + 1);
                Ok(Value::Num(position as f64))
            }
            "split" => {
                if arguments.len() < 2 {
                    return Err(AwkError::Usage);
                }
                let value = self.eval_string(&arguments[0])?;
                let Expr::Var(array) = &arguments[1] else {
                    return Err(AwkError::Usage);
                };
                let separator = if let Some(separator) = arguments.get(2) {
                    self.eval_string(separator)?
                } else {
                    self.get_string("FS")
                };
                let parts = split_fields(&value, &separator);
                let mut map = BTreeMap::new();
                for (index, part) in parts.iter().enumerate() {
                    map.insert((index + 1).to_string(), Value::Str(part.clone()));
                }
                self.vars.insert(array.clone(), Value::Array(map));
                Ok(Value::Num(parts.len() as f64))
            }
            "int" => {
                if arguments.len() != 1 {
                    return Err(AwkError::Usage);
                }
                let value = self.eval_number(&arguments[0])?;
                Ok(Value::Num(value as i64 as f64))
            }
            "sprintf" => {
                if arguments.is_empty() {
                    return Err(AwkError::Usage);
                }
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(self.eval(argument)?);
                }
                Ok(Value::Str(format_values(&mut values)?))
            }
            "match" => {
                if arguments.len() != 2 {
                    return Err(AwkError::Usage);
                }
                let value = self.eval_string(&arguments[0])?;
                let pattern = self.pattern(&arguments[1])?;
                let regex = compile(&pattern)?;
                let mut slots = vec![RegMatch::default(); regex.nsub().saturating_add(1)];
                match regexec(&regex, value.as_bytes(), &mut slots, 0, &ByteLocale) {
                    Ok(()) => {
                        let start = slots[0].rm_so.max(0) as usize;
                        let end = slots[0].rm_eo.max(0) as usize;
                        self.set_var(
                            "RSTART",
                            Value::Num((value[..start].chars().count() + 1) as f64),
                        );
                        self.set_var(
                            "RLENGTH",
                            Value::Num(value[start..end].chars().count() as f64),
                        );
                        Ok(Value::Num((value[..start].chars().count() + 1) as f64))
                    }
                    Err(error) if error.0 == REG_NOMATCH => {
                        self.set_var("RSTART", Value::Num(0.0));
                        self.set_var("RLENGTH", Value::Num(-1.0));
                        Ok(Value::Num(0.0))
                    }
                    Err(_) => Err(AwkError::Runtime),
                }
            }
            "sub" | "gsub" => {
                if arguments.len() < 2 {
                    return Err(AwkError::Usage);
                }
                let pattern = self.pattern(&arguments[0])?;
                let replacement = self.eval_string(&arguments[1])?;
                let (text, changed) = if let Some(target) = arguments.get(2) {
                    (self.eval_string(target)?, Some(target))
                } else {
                    (self.record.clone(), None)
                };
                let (result, count) = substitute(&text, &pattern, &replacement, name == "gsub")?;
                if let Some(target) = changed {
                    self.assign(target, Value::Str(result))?;
                } else if count > 0 {
                    self.set_record(&result);
                }
                Ok(Value::Num(count as f64))
            }
            "tolower" => {
                let value = self.eval_string(arguments.first().ok_or(AwkError::Usage)?)?;
                Ok(Value::Str(value.to_lowercase()))
            }
            "toupper" => {
                let value = self.eval_string(arguments.first().ok_or(AwkError::Usage)?)?;
                Ok(Value::Str(value.to_uppercase()))
            }
            "getline" | "close" | "system" => Err(AwkError::Usage),
            _ => Err(AwkError::Usage),
        }
    }

    fn pattern(&mut self, expression: &Expr) -> Result<String, AwkError> {
        match expression {
            Expr::Regex(pattern) => Ok(pattern.clone()),
            _ => Ok(self.eval_string(expression)?),
        }
    }

    fn read_lvalue(&mut self, expression: &Expr) -> Result<Value, AwkError> {
        self.eval(expression)
    }

    fn assign(&mut self, expression: &Expr, value: Value) -> Result<(), AwkError> {
        match expression {
            Expr::Var(name) => {
                self.set_var(name, value);
                Ok(())
            }
            Expr::Field(index) => {
                let index = self.eval_number(index)? as usize;
                let text = self.to_string(&value);
                self.set_field(index, text);
                Ok(())
            }
            Expr::Index(name, index) => {
                let key = self.eval_string(index)?;
                self.get_array(name).insert(key, value);
                Ok(())
            }
            _ => Err(AwkError::Usage),
        }
    }

    fn get_var(&mut self, name: &str) -> Value {
        self.vars
            .get(name)
            .cloned()
            .unwrap_or(Value::Str(String::new()))
    }

    fn set_var(&mut self, name: &str, value: Value) {
        self.vars.insert(name.to_string(), value);
    }

    fn get_array(&mut self, name: &str) -> &mut BTreeMap<String, Value> {
        if !matches!(self.vars.get(name), Some(Value::Array(_))) {
            self.vars
                .insert(name.to_string(), Value::Array(BTreeMap::new()));
        }
        if let Some(Value::Array(values)) = self.vars.get_mut(name) {
            return values;
        }
        unreachable!()
    }

    fn get_field(&self, index: usize) -> String {
        if index == 0 {
            self.record.clone()
        } else {
            self.fields
                .get(index.saturating_sub(1))
                .cloned()
                .unwrap_or_default()
        }
    }

    fn set_record(&mut self, record: &str) {
        self.record = record.to_string();
        let fs = self.get_string("FS");
        self.fields = split_fields(record, &fs);
        self.set_var("NF", Value::Num(self.fields.len() as f64));
    }

    fn set_field(&mut self, index: usize, value: String) {
        if index == 0 {
            self.set_record(&value);
            return;
        }
        while self.fields.len() < index {
            self.fields.push(String::new());
        }
        if let Some(slot) = self.fields.get_mut(index.saturating_sub(1)) {
            *slot = value;
        }
        let ofs = self.get_string("OFS");
        self.record = self.fields.join(&ofs);
        self.set_var("NF", Value::Num(self.fields.len() as f64));
    }

    fn print_values(&mut self, values: &[Expr]) {
        let ofs = self.get_string("OFS");
        let ors = self.get_string("ORS");
        let text = if values.is_empty() {
            self.record.clone()
        } else {
            let mut parts = Vec::new();
            for value in values {
                if let Ok(value) = self.eval(value) {
                    parts.push(self.to_string(&value));
                }
            }
            parts.join(&ofs)
        };
        self.output.push_str(&text);
        self.output.push_str(&ors);
    }

    fn printf_values(&mut self, values: &[Expr]) {
        let mut evaluated = Vec::new();
        for value in values {
            if let Ok(value) = self.eval(value) {
                evaluated.push(value);
            }
        }
        if let Ok(text) = format_values(&mut evaluated) {
            self.output.push_str(&text);
        }
    }

    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Num(number) => *number != 0.0,
            Value::Str(text) => !text.is_empty(),
            Value::Array(values) => !values.is_empty(),
        }
    }

    fn to_number(&self, value: &Value) -> f64 {
        match value {
            Value::Num(number) => *number,
            Value::Str(text) => text.trim().parse().unwrap_or(0.0),
            Value::Array(_) => 0.0,
        }
    }

    fn to_string(&self, value: &Value) -> String {
        match value {
            Value::Num(number) => number_to_string(*number),
            Value::Str(text) => text.clone(),
            Value::Array(_) => String::new(),
        }
    }

    fn compare(&mut self, left: &Value, right: &Value) -> core::cmp::Ordering {
        let left_number = self.numeric_value(left);
        let right_number = self.numeric_value(right);
        if let (Some(left), Some(right)) = (left_number, right_number) {
            left.partial_cmp(&right)
                .unwrap_or(core::cmp::Ordering::Equal)
        } else {
            self.to_string(left).cmp(&self.to_string(right))
        }
    }

    fn numeric_value(&mut self, value: &Value) -> Option<f64> {
        match value {
            Value::Num(number) => Some(*number),
            Value::Str(text) => text.trim().parse().ok(),
            Value::Array(_) => None,
        }
    }
}

enum Control {
    None,
    Break,
    Continue,
    Next,
}

fn number_to_string(number: f64) -> String {
    let integer = number as i64 as f64;
    if integer == number {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

fn power(base: f64, exponent: f64) -> f64 {
    let integer = exponent as i64;
    if integer as f64 != exponent {
        return 0.0;
    }
    let mut result = 1.0f64;
    let mut count = if integer < 0 { -integer } else { integer };
    while count > 0 {
        result *= base;
        count -= 1;
    }
    if integer < 0 && result != 0.0 {
        1.0 / result
    } else {
        result
    }
}

fn split_fields(record: &str, separator: &str) -> Vec<String> {
    if separator.is_empty() || separator == " " {
        return record.split_whitespace().map(str::to_string).collect();
    }
    if separator.chars().count() == 1 {
        let delimiter = separator.chars().next().unwrap_or(' ');
        return record.split(delimiter).map(str::to_string).collect();
    }
    record.split(separator).map(str::to_string).collect()
}

fn compile(pattern: &str) -> Result<Regex, AwkError> {
    regcomp(pattern.as_bytes(), REG_BASIC, &ByteLocale).map_err(|_| AwkError::Usage)
}

fn regex_matches(regex: &Regex, text: &[u8]) -> Result<bool, AwkError> {
    match regexec(regex, text, &mut [], 0, &ByteLocale) {
        Ok(()) => Ok(true),
        Err(error) if error.0 == REG_NOMATCH => Ok(false),
        Err(_) => Err(AwkError::Runtime),
    }
}

fn substitute(
    text: &str,
    pattern: &str,
    replacement: &str,
    global: bool,
) -> Result<(String, usize), AwkError> {
    let regex = compile(pattern)?;
    let subject = text.as_bytes();
    let mut slots = vec![RegMatch::default(); regex.nsub().saturating_add(1)];
    let mut output = Vec::new();
    let mut copied = 0usize;
    let mut cursor = 0usize;
    let mut count = 0usize;
    while cursor <= subject.len() {
        if regexec(&regex, &subject[cursor..], &mut slots, 0, &ByteLocale).is_err() {
            break;
        }
        let start = cursor.saturating_add(slots[0].rm_so.max(0) as usize);
        let end = cursor.saturating_add(slots[0].rm_eo.max(0) as usize);
        if end < start || end > subject.len() {
            break;
        }
        output.extend_from_slice(subject.get(copied..start).unwrap_or(&[]));
        output.extend(replacement_bytes(replacement, &subject[cursor..], &slots));
        copied = end;
        count = count.saturating_add(1);
        if !global {
            break;
        }
        if end == start {
            if end >= subject.len() {
                break;
            }
            let next = end.saturating_add(1);
            output.extend_from_slice(subject.get(end..next).unwrap_or(&[]));
            copied = next;
            cursor = next;
        } else {
            cursor = end;
        }
    }
    output.extend_from_slice(subject.get(copied..).unwrap_or(&[]));
    Ok((String::from_utf8_lossy(&output).into_owned(), count))
}

fn replacement_bytes(replacement: &str, subject: &[u8], slots: &[RegMatch]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut chars = replacement.chars();
    while let Some(character) = chars.next() {
        if character == '&' {
            if let Some(slot) = slots.first()
                && slot.rm_so >= 0
                && slot.rm_eo >= slot.rm_so
            {
                output.extend_from_slice(
                    subject
                        .get(slot.rm_so as usize..slot.rm_eo as usize)
                        .unwrap_or(&[]),
                );
            }
        } else if character == '\\' {
            if let Some(next) = chars.next() {
                output.extend_from_slice(next.to_string().as_bytes());
            }
        } else {
            output.extend_from_slice(character.to_string().as_bytes());
        }
    }
    output
}

fn format_values(values: &mut [Value]) -> Result<String, AwkError> {
    let Some(Value::Str(format)) = values.first() else {
        return Err(AwkError::Usage);
    };
    let format = format.clone();
    let mut output = String::new();
    let mut arguments = values.iter().skip(1);
    let mut chars = format.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '%' {
            output.push(character);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            output.push('%');
            continue;
        }
        let mut spec = String::from("%");
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_digit() || matches!(next, '.' | '-' | '+' | ' ' | '#') {
                spec.push(next);
                chars.next();
            } else {
                break;
            }
        }
        let Some(conversion) = chars.next() else {
            return Err(AwkError::Usage);
        };
        let value = arguments
            .next()
            .cloned()
            .unwrap_or(Value::Str(String::new()));
        match conversion {
            's' => {
                let text = value_string(&value);
                output.push_str(&apply_width(&format!("{spec}s"), &text));
            }
            'd' | 'i' => {
                let number = value_number(&value);
                output.push_str(&apply_width(
                    &format!("{spec}d"),
                    &(number as i64).to_string(),
                ));
            }
            'f' | 'g' => {
                let number = value_number(&value);
                output.push_str(&apply_width(
                    &format!("{spec}{conversion}"),
                    &number.to_string(),
                ));
            }
            'c' => {
                let text = value_string(&value);
                if let Some(character) = text.chars().next() {
                    output.push(character);
                }
            }
            _ => return Err(AwkError::Usage),
        }
    }
    Ok(output)
}

fn apply_width(spec: &str, value: &str) -> String {
    let width = spec
        .trim_start_matches('%')
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .parse::<usize>()
        .unwrap_or(0);
    if width > value.chars().count() {
        let mut result = String::new();
        result.push_str(&" ".repeat(width - value.chars().count()));
        result.push_str(value);
        result
    } else {
        value.to_string()
    }
}

fn value_string(value: &Value) -> String {
    match value {
        Value::Num(number) => number_to_string(*number),
        Value::Str(text) => text.clone(),
        Value::Array(_) => String::new(),
    }
}

fn value_number(value: &Value) -> f64 {
    match value {
        Value::Num(number) => *number,
        Value::Str(text) => text.trim().parse().unwrap_or(0.0),
        Value::Array(_) => 0.0,
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
