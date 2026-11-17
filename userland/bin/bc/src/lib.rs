// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `bc` calculator.
//!
//! The implementation provides arbitrary-precision decimal arithmetic, the POSIX statement and
//! function core, and a bounded `-l` math library. The `dc` language and GNU-only extensions are
//! explicit unsupported cases.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

mod math;
mod number;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use number::Number;

const MAX_STEPS: usize = 5_000_000;
const MAX_CALL_DEPTH: usize = 64;
const MAX_ARRAY_INDEX: i64 = 1_000_000;
const MAX_SCALE: usize = 1_000_000;

/// Why a `bc` program was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BcError {
    /// The input could not be tokenized or parsed.
    Parse(String),
    /// Evaluation failed (division by zero, undefined function, bounds, ...).
    Runtime(String),
    /// A construct is deliberately outside the bounded core.
    Unsupported(String),
}

impl core::fmt::Display for BcError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "parse error: {message}"),
            Self::Runtime(message) => write!(formatter, "runtime error: {message}"),
            Self::Unsupported(message) => write!(formatter, "unsupported: {message}"),
        }
    }
}

/// Command-line options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `-l`: set `scale` to 20 and enable the bounded math library.
    pub math_lib: bool,
}

/// A persistent `bc` session. Definitions and variables survive [`Session::process`] calls.
pub struct Session {
    runtime: Runtime,
}

impl Session {
    /// Create a session with `options`.
    #[must_use]
    pub fn new(options: &Options) -> Self {
        Self {
            runtime: Runtime::new(options.math_lib),
        }
    }

    /// Parse and execute one input source.
    pub fn process(&mut self, input: &str) -> Result<(), BcError> {
        if self.runtime.quit {
            return Ok(());
        }
        let tokens = tokenize(input)?;
        let program = Parser::new(tokens).parse_program()?;
        self.runtime.run_program(&program)
    }

    /// Whether the input executed `quit`.
    #[must_use]
    pub fn quit(&self) -> bool {
        self.runtime.quit
    }

    /// Consume the session and return everything written to standard output.
    #[must_use]
    pub fn finish(self) -> String {
        self.runtime.output
    }
}

/// Run one source through a fresh session.
pub fn run(input: &str, options: &Options) -> Result<String, BcError> {
    let mut session = Session::new(options);
    session.process(input)?;
    Ok(session.finish())
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(Number),
    Ident(String),
    Str(String),
    Op(&'static str),
    Newline,
    Eof,
}

static EOF_TOKEN: Token = Token::Eof;

fn tokenize(input: &str) -> Result<Vec<Token>, BcError> {
    let characters: Vec<char> = input.chars().collect();
    let mut position = 0usize;
    let mut tokens = Vec::new();
    while position < characters.len() {
        let character = characters.get(position).copied().unwrap_or(' ');
        match character {
            ' ' | '\t' | '\r' => {
                position = position.saturating_add(1);
            }
            '\n' => {
                tokens.push(Token::Newline);
                position = position.saturating_add(1);
            }
            '\\' if characters.get(position + 1) == Some(&'\n') => {
                position = position.saturating_add(2);
            }
            '#' => {
                while position < characters.len() && characters.get(position) != Some(&'\n') {
                    position = position.saturating_add(1);
                }
            }
            '/' if characters.get(position + 1) == Some(&'*') => {
                position = position.saturating_add(2);
                let mut closed = false;
                while position < characters.len() {
                    if characters.get(position) == Some(&'*')
                        && characters.get(position + 1) == Some(&'/')
                    {
                        position = position.saturating_add(2);
                        closed = true;
                        break;
                    }
                    position = position.saturating_add(1);
                }
                if !closed {
                    return Err(BcError::Parse("unterminated comment".to_string()));
                }
            }
            '"' => {
                position = position.saturating_add(1);
                let mut value = String::new();
                let mut closed = false;
                while position < characters.len() {
                    let character = characters.get(position).copied().unwrap_or(' ');
                    position = position.saturating_add(1);
                    if character == '"' {
                        closed = true;
                        break;
                    }
                    if character == '\\' {
                        let Some(next) = characters.get(position).copied() else {
                            return Err(BcError::Parse("unterminated string".to_string()));
                        };
                        position = position.saturating_add(1);
                        match next {
                            'n' => value.push('\n'),
                            't' => value.push('\t'),
                            'r' => value.push('\r'),
                            'b' => value.push('\u{8}'),
                            'f' => value.push('\u{c}'),
                            '\\' => value.push('\\'),
                            '"' => value.push('"'),
                            '\n' => {}
                            other => value.push(other),
                        }
                    } else {
                        value.push(character);
                    }
                }
                if !closed {
                    return Err(BcError::Parse("unterminated string".to_string()));
                }
                tokens.push(Token::Str(value));
            }
            character
                if character.is_ascii_digit()
                    || (character == '.'
                        && characters
                            .get(position + 1)
                            .is_some_and(|next| next.is_ascii_digit())) =>
            {
                let start = position;
                let mut seen_dot = false;
                while position < characters.len() {
                    let character = characters.get(position).copied().unwrap_or(' ');
                    if character.is_ascii_digit() {
                        position = position.saturating_add(1);
                    } else if character == '.' && !seen_dot {
                        seen_dot = true;
                        position = position.saturating_add(1);
                    } else {
                        break;
                    }
                }
                let text: String = characters
                    .get(start..position)
                    .unwrap_or(&[])
                    .iter()
                    .collect();
                let number = Number::parse_decimal(&text)
                    .ok_or_else(|| BcError::Parse(format!("bad number `{text}`")))?;
                tokens.push(Token::Number(number));
            }
            character if character.is_ascii_alphabetic() || character == '_' => {
                let start = position;
                while position < characters.len() {
                    let character = characters.get(position).copied().unwrap_or(' ');
                    if character.is_ascii_alphanumeric() || character == '_' {
                        position = position.saturating_add(1);
                    } else {
                        break;
                    }
                }
                let name: String = characters
                    .get(start..position)
                    .unwrap_or(&[])
                    .iter()
                    .collect();
                tokens.push(Token::Ident(name));
            }
            _ => {
                let remaining: String = characters
                    .get(position..position.saturating_add(2))
                    .unwrap_or(&[])
                    .iter()
                    .collect();
                let mut matched = false;
                for operator in [
                    "+=", "-=", "*=", "/=", "%=", "^=", "==", "!=", "<=", ">=", "&&", "||",
                ] {
                    if remaining.starts_with(operator) {
                        tokens.push(Token::Op(operator));
                        position = position.saturating_add(2);
                        matched = true;
                        break;
                    }
                }
                if matched {
                    continue;
                }
                let operator = match character {
                    '+' => "+",
                    '-' => "-",
                    '*' => "*",
                    '/' => "/",
                    '%' => "%",
                    '^' => "^",
                    '=' => "=",
                    '<' => "<",
                    '>' => ">",
                    '!' => "!",
                    '(' => "(",
                    ')' => ")",
                    '[' => "[",
                    ']' => "]",
                    '{' => "{",
                    '}' => "}",
                    ',' => ",",
                    ';' => ";",
                    _ => {
                        return Err(BcError::Parse(format!(
                            "unexpected character `{character}`"
                        )));
                    }
                };
                tokens.push(Token::Op(operator));
                position = position.saturating_add(1);
            }
        }
    }
    tokens.push(Token::Eof);
    Ok(tokens)
}

#[derive(Clone, Debug)]
enum Expr {
    Number(Number),
    Str(String),
    Var(String),
    Array(String, Box<Expr>),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Debug)]
enum Stmt {
    Expr(Expr),
    Print(Vec<Expr>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    For(Option<Expr>, Option<Expr>, Option<Expr>, Box<Stmt>),
    Block(Vec<Stmt>),
    Break,
    Continue,
    Return(Option<Expr>),
    Quit,
    Define(Function),
    Auto(Vec<String>),
}

#[derive(Clone, Debug)]
struct Function {
    name: String,
    parameters: Vec<String>,
    body: Vec<Stmt>,
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
        self.tokens.get(self.position).unwrap_or(&EOF_TOKEN)
    }

    fn next(&mut self) -> Token {
        let token = self.peek().clone();
        self.position = self.position.saturating_add(1);
        token
    }

    fn peek_op(&self, operator: &str) -> bool {
        matches!(self.peek(), Token::Op(value) if *value == operator)
    }

    fn peek_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Token::Ident(value) if value == name)
    }

    fn eat_op(&mut self, operator: &str) -> bool {
        if self.peek_op(operator) {
            self.position = self.position.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, operator: &str) -> Result<(), BcError> {
        if self.eat_op(operator) {
            Ok(())
        } else {
            Err(BcError::Parse(format!("expected `{operator}`")))
        }
    }

    fn skip_separators(&mut self) {
        while matches!(self.peek(), Token::Newline) || self.peek_op(";") {
            self.position = self.position.saturating_add(1);
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Token::Newline) {
            self.position = self.position.saturating_add(1);
        }
    }

    fn parse_program(mut self) -> Result<Vec<Stmt>, BcError> {
        let mut statements = Vec::new();
        loop {
            self.skip_separators();
            if matches!(self.peek(), Token::Eof) {
                break;
            }
            statements.push(self.parse_statement()?);
            if !matches!(self.peek(), Token::Newline | Token::Eof) && !self.peek_op(";") {
                return Err(BcError::Parse("expected a statement separator".to_string()));
            }
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Stmt, BcError> {
        if self.peek_op("{") {
            return Ok(Stmt::Block(self.parse_block()?));
        }
        if self.peek_ident("if") {
            return self.parse_if();
        }
        if self.peek_ident("while") {
            self.next();
            return self.parse_while();
        }
        if self.peek_ident("for") {
            self.next();
            return self.parse_for();
        }
        if self.peek_ident("define") {
            return self.parse_define();
        }
        if self.peek_ident("print") {
            self.next();
            return self.parse_print();
        }
        if self.peek_ident("return") {
            self.next();
            return self.parse_return();
        }
        if self.peek_ident("break") {
            self.next();
            return Ok(Stmt::Break);
        }
        if self.peek_ident("continue") {
            self.next();
            return Ok(Stmt::Continue);
        }
        if self.peek_ident("quit") {
            self.next();
            return Ok(Stmt::Quit);
        }
        if self.peek_ident("auto") {
            self.next();
            return self.parse_auto();
        }
        if self.peek_ident("else") {
            return Err(BcError::Parse("unexpected `else`".to_string()));
        }
        Ok(Stmt::Expr(self.parse_expr()?))
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, BcError> {
        self.expect_op("{")?;
        let mut statements = Vec::new();
        loop {
            self.skip_separators();
            if self.eat_op("}") {
                break;
            }
            if matches!(self.peek(), Token::Eof) {
                return Err(BcError::Parse("unterminated block".to_string()));
            }
            statements.push(self.parse_statement()?);
            if !matches!(self.peek(), Token::Newline | Token::Eof)
                && !self.peek_op(";")
                && !self.peek_op("}")
            {
                return Err(BcError::Parse("expected a statement separator".to_string()));
            }
        }
        Ok(statements)
    }

    fn parse_if(&mut self) -> Result<Stmt, BcError> {
        self.next();
        self.expect_op("(")?;
        self.skip_newlines();
        let condition = self.parse_expr()?;
        self.skip_newlines();
        self.expect_op(")")?;
        self.skip_newlines();
        let then_branch = Box::new(self.parse_statement()?);
        self.skip_newlines();
        let otherwise = if self.peek_ident("else") {
            self.next();
            self.skip_newlines();
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };
        Ok(Stmt::If(condition, then_branch, otherwise))
    }

    fn parse_while(&mut self) -> Result<Stmt, BcError> {
        self.expect_op("(")?;
        self.skip_newlines();
        let condition = self.parse_expr()?;
        self.skip_newlines();
        self.expect_op(")")?;
        self.skip_newlines();
        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::While(condition, body))
    }

    fn parse_for(&mut self) -> Result<Stmt, BcError> {
        self.expect_op("(")?;
        self.skip_newlines();
        let init = if self.peek_op(";") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_op(";")?;
        self.skip_newlines();
        let condition = if self.peek_op(";") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_op(";")?;
        self.skip_newlines();
        let increment = if self.peek_op(")") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.skip_newlines();
        self.expect_op(")")?;
        self.skip_newlines();
        let body = Box::new(self.parse_statement()?);
        Ok(Stmt::For(init, condition, increment, body))
    }

    fn parse_define(&mut self) -> Result<Stmt, BcError> {
        self.next();
        let name = match self.next() {
            Token::Ident(name) => name,
            _ => return Err(BcError::Parse("expected a function name".to_string())),
        };
        self.expect_op("(")?;
        let mut parameters = Vec::new();
        if !self.peek_op(")") {
            loop {
                let parameter = match self.next() {
                    Token::Ident(name) => name,
                    _ => return Err(BcError::Parse("expected a parameter".to_string())),
                };
                if self.peek_op("[") {
                    return Err(BcError::Unsupported(
                        "array parameters are not implemented".to_string(),
                    ));
                }
                parameters.push(parameter);
                if !self.eat_op(",") {
                    break;
                }
            }
        }
        self.expect_op(")")?;
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(Stmt::Define(Function {
            name,
            parameters,
            body,
        }))
    }

    fn parse_print(&mut self) -> Result<Stmt, BcError> {
        let mut values = Vec::new();
        if matches!(self.peek(), Token::Newline | Token::Eof)
            || self.peek_op(";")
            || self.peek_op("}")
        {
            return Ok(Stmt::Print(values));
        }
        loop {
            values.push(self.parse_expr()?);
            if !self.eat_op(",") {
                break;
            }
            self.skip_newlines();
        }
        Ok(Stmt::Print(values))
    }

    fn parse_return(&mut self) -> Result<Stmt, BcError> {
        if self.eat_op("(") {
            self.skip_newlines();
            let value = self.parse_expr()?;
            self.skip_newlines();
            self.expect_op(")")?;
            return Ok(Stmt::Return(Some(value)));
        }
        if matches!(self.peek(), Token::Newline | Token::Eof)
            || self.peek_op(";")
            || self.peek_op("}")
        {
            return Ok(Stmt::Return(None));
        }
        Ok(Stmt::Return(Some(self.parse_expr()?)))
    }

    fn parse_auto(&mut self) -> Result<Stmt, BcError> {
        let mut names = Vec::new();
        loop {
            let name = match self.next() {
                Token::Ident(name) => name,
                _ => return Err(BcError::Parse("expected an auto variable".to_string())),
            };
            if self.peek_op("[") {
                return Err(BcError::Unsupported(
                    "auto arrays are not implemented".to_string(),
                ));
            }
            names.push(name);
            if !self.eat_op(",") {
                break;
            }
        }
        Ok(Stmt::Auto(names))
    }

    fn parse_expr(&mut self) -> Result<Expr, BcError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, BcError> {
        let left = self.parse_or()?;
        for operator in ["=", "+=", "-=", "*=", "/=", "%=", "^="] {
            if self.eat_op(operator) {
                let right = self.parse_assignment()?;
                if !is_lvalue(&left) {
                    return Err(BcError::Parse("assignment to a non-lvalue".to_string()));
                }
                return Ok(Expr::Assign(operator, Box::new(left), Box::new(right)));
            }
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<Expr, BcError> {
        let mut left = self.parse_and()?;
        while self.eat_op("||") {
            self.skip_newlines();
            let right = self.parse_and()?;
            left = Expr::Binary("||", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, BcError> {
        let mut left = self.parse_comparison()?;
        while self.eat_op("&&") {
            self.skip_newlines();
            let right = self.parse_comparison()?;
            left = Expr::Binary("&&", Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, BcError> {
        let mut left = self.parse_add()?;
        loop {
            let operator = match self.peek() {
                Token::Op(value @ ("==" | "!=" | "<" | "<=" | ">" | ">=")) => Some(*value),
                _ => None,
            };
            let Some(operator) = operator else {
                return Ok(left);
            };
            self.next();
            self.skip_newlines();
            let right = self.parse_add()?;
            left = Expr::Binary(operator, Box::new(left), Box::new(right));
        }
    }

    fn parse_add(&mut self) -> Result<Expr, BcError> {
        let mut left = self.parse_mul()?;
        loop {
            let operator = match self.peek() {
                Token::Op(value @ ("+" | "-")) => Some(*value),
                _ => None,
            };
            let Some(operator) = operator else {
                return Ok(left);
            };
            self.next();
            self.skip_newlines();
            let right = self.parse_mul()?;
            left = Expr::Binary(operator, Box::new(left), Box::new(right));
        }
    }

    fn parse_mul(&mut self) -> Result<Expr, BcError> {
        let mut left = self.parse_unary()?;
        loop {
            let operator = match self.peek() {
                Token::Op(value @ ("*" | "/" | "%")) => Some(*value),
                _ => None,
            };
            let Some(operator) = operator else {
                return Ok(left);
            };
            self.next();
            self.skip_newlines();
            let right = self.parse_unary()?;
            left = Expr::Binary(operator, Box::new(left), Box::new(right));
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, BcError> {
        if self.peek_op("-") || self.peek_op("+") || self.peek_op("!") {
            let operator = match self.next() {
                Token::Op(operator) => operator,
                _ => return Err(BcError::Parse("expected a unary operator".to_string())),
            };
            let value = self.parse_unary()?;
            return Ok(Expr::Unary(operator, Box::new(value)));
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> Result<Expr, BcError> {
        let left = self.parse_primary()?;
        if self.eat_op("^") {
            self.skip_newlines();
            let right = self.parse_unary()?;
            return Ok(Expr::Binary("^", Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr, BcError> {
        match self.next() {
            Token::Number(number) => Ok(Expr::Number(number)),
            Token::Str(value) => Ok(Expr::Str(value)),
            Token::Ident(name) => {
                if self.eat_op("(") {
                    let mut arguments = Vec::new();
                    self.skip_newlines();
                    if !self.peek_op(")") {
                        loop {
                            arguments.push(self.parse_expr()?);
                            self.skip_newlines();
                            if !self.eat_op(",") {
                                break;
                            }
                            self.skip_newlines();
                        }
                    }
                    self.expect_op(")")?;
                    Ok(Expr::Call(name, arguments))
                } else if self.eat_op("[") {
                    self.skip_newlines();
                    let index = self.parse_expr()?;
                    self.skip_newlines();
                    self.expect_op("]")?;
                    Ok(Expr::Array(name, Box::new(index)))
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Token::Op("(") => {
                self.skip_newlines();
                let value = self.parse_expr()?;
                self.skip_newlines();
                self.expect_op(")")?;
                Ok(value)
            }
            _ => Err(BcError::Parse("expected an expression".to_string())),
        }
    }
}

fn is_lvalue(expression: &Expr) -> bool {
    matches!(expression, Expr::Var(_) | Expr::Array(_, _))
}

#[derive(Clone, Debug)]
enum Value {
    Number(Number),
    Str(String),
}

#[derive(Clone, Debug)]
enum Action {
    None,
    Break,
    Continue,
    Return(Number),
    Quit,
}

struct Frame {
    variables: BTreeMap<String, Number>,
}

struct Runtime {
    globals: BTreeMap<String, Number>,
    arrays: BTreeMap<String, BTreeMap<i64, Number>>,
    functions: BTreeMap<String, Function>,
    frames: Vec<Frame>,
    scale: usize,
    math_lib: bool,
    output: String,
    steps: usize,
    quit: bool,
}

impl Runtime {
    fn new(math_lib: bool) -> Self {
        Self {
            globals: BTreeMap::new(),
            arrays: BTreeMap::new(),
            functions: BTreeMap::new(),
            frames: Vec::new(),
            scale: if math_lib { 20 } else { 0 },
            math_lib,
            output: String::new(),
            steps: 0,
            quit: false,
        }
    }

    fn run_program(&mut self, statements: &[Stmt]) -> Result<(), BcError> {
        for statement in statements {
            match self.exec_stmt(statement)? {
                Action::None => {}
                Action::Quit => {
                    self.quit = true;
                    break;
                }
                Action::Break | Action::Continue => {
                    return Err(BcError::Runtime(
                        "break or continue outside a loop".to_string(),
                    ));
                }
                Action::Return(_) => {
                    return Err(BcError::Runtime("return outside a function".to_string()));
                }
            }
            if self.quit {
                break;
            }
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), BcError> {
        self.steps = self.steps.saturating_add(1);
        if self.steps > MAX_STEPS {
            return Err(BcError::Runtime("operation limit exceeded".to_string()));
        }
        Ok(())
    }

    fn exec_stmt(&mut self, statement: &Stmt) -> Result<Action, BcError> {
        self.tick()?;
        match statement {
            Stmt::Expr(expression) => {
                let value = self.eval(expression)?;
                if !matches!(expression, Expr::Assign(..)) {
                    self.emit(&value, true);
                }
                Ok(Action::None)
            }
            Stmt::Print(values) => {
                for expression in values {
                    let value = self.eval(expression)?;
                    self.emit(&value, false);
                }
                Ok(Action::None)
            }
            Stmt::If(condition, then_branch, otherwise) => {
                let condition = self.eval(condition)?;
                if self.truthy(&condition) {
                    self.exec_stmt(then_branch)
                } else if let Some(otherwise) = otherwise {
                    self.exec_stmt(otherwise)
                } else {
                    Ok(Action::None)
                }
            }
            Stmt::While(condition, body) => {
                let mut iterations = 0usize;
                loop {
                    let condition = self.eval(condition)?;
                    if !self.truthy(&condition) {
                        break;
                    }
                    iterations = iterations.saturating_add(1);
                    if iterations > MAX_STEPS {
                        return Err(BcError::Runtime("loop limit exceeded".to_string()));
                    }
                    match self.exec_stmt(body)? {
                        Action::Break => break,
                        Action::Continue | Action::None => {}
                        action => return Ok(action),
                    }
                    if self.quit {
                        return Ok(Action::Quit);
                    }
                }
                Ok(Action::None)
            }
            Stmt::For(init, condition, increment, body) => {
                if let Some(init) = init {
                    self.eval(init)?;
                }
                let mut iterations = 0usize;
                loop {
                    if let Some(condition) = condition {
                        let condition = self.eval(condition)?;
                        if !self.truthy(&condition) {
                            break;
                        }
                    }
                    iterations = iterations.saturating_add(1);
                    if iterations > MAX_STEPS {
                        return Err(BcError::Runtime("loop limit exceeded".to_string()));
                    }
                    match self.exec_stmt(body)? {
                        Action::Break => break,
                        Action::Continue | Action::None => {}
                        action => return Ok(action),
                    }
                    if self.quit {
                        return Ok(Action::Quit);
                    }
                    if let Some(increment) = increment {
                        self.eval(increment)?;
                    }
                }
                Ok(Action::None)
            }
            Stmt::Block(statements) => {
                for statement in statements {
                    let action = self.exec_stmt(statement)?;
                    if !matches!(action, Action::None) {
                        return Ok(action);
                    }
                    if self.quit {
                        return Ok(Action::Quit);
                    }
                }
                Ok(Action::None)
            }
            Stmt::Break => Ok(Action::Break),
            Stmt::Continue => Ok(Action::Continue),
            Stmt::Return(value) => {
                if self.frames.is_empty() {
                    return Err(BcError::Runtime("return outside a function".to_string()));
                }
                let value = match value {
                    Some(value) => {
                        let value = self.eval(value)?;
                        self.value_number(&value)?
                    }
                    None => Number::zero(),
                };
                Ok(Action::Return(value))
            }
            Stmt::Quit => Ok(Action::Quit),
            Stmt::Define(function) => {
                self.functions
                    .insert(function.name.clone(), function.clone());
                Ok(Action::None)
            }
            Stmt::Auto(names) => {
                let Some(frame) = self.frames.last_mut() else {
                    return Err(BcError::Runtime("auto outside a function".to_string()));
                };
                for name in names {
                    frame
                        .variables
                        .entry(name.clone())
                        .or_insert_with(Number::zero);
                }
                Ok(Action::None)
            }
        }
    }

    fn eval(&mut self, expression: &Expr) -> Result<Value, BcError> {
        self.tick()?;
        match expression {
            Expr::Number(number) => Ok(Value::Number(number.clone())),
            Expr::Str(value) => Ok(Value::Str(value.clone())),
            Expr::Var(name) => Ok(Value::Number(self.get_variable(name))),
            Expr::Array(name, index) => {
                let index = self.array_index(index)?;
                Ok(Value::Number(self.get_array(name, index)))
            }
            Expr::Unary(operator, value) => {
                let value = self.eval(value)?;
                let value = self.value_number(&value)?;
                match *operator {
                    "-" => Ok(Value::Number(value.negated())),
                    "+" => Ok(Value::Number(value)),
                    "!" => Ok(Value::Number(Number::from_i64(i64::from(value.is_zero())))),
                    _ => Err(BcError::Runtime("unknown unary operator".to_string())),
                }
            }
            Expr::Binary(operator, left, right) => self.eval_binary(operator, left, right),
            Expr::Assign(operator, left, right) => {
                let right = self.eval(right)?;
                let value = if *operator == "=" {
                    right
                } else {
                    let left_value = self.read_lvalue(left)?;
                    let left_number = self.value_number(&left_value)?;
                    let right_number = self.value_number(&right)?;
                    let result = match *operator {
                        "+=" => left_number.add(&right_number),
                        "-=" => left_number.sub(&right_number),
                        "*=" => left_number.mul(&right_number, self.scale),
                        "/=" => left_number
                            .div(&right_number, self.scale)
                            .ok_or_else(|| BcError::Runtime("division by zero".to_string()))?,
                        "%=" => left_number
                            .rem(&right_number, self.scale)
                            .ok_or_else(|| BcError::Runtime("division by zero".to_string()))?,
                        "^=" => left_number
                            .pow(&right_number, self.scale)
                            .ok_or_else(|| BcError::Runtime("invalid exponent".to_string()))?,
                        _ => return Err(BcError::Runtime("unknown assignment".to_string())),
                    };
                    Value::Number(result)
                };
                self.assign(left, value.clone())?;
                Ok(value)
            }
            Expr::Call(name, arguments) => self.call(name, arguments),
        }
    }

    fn eval_binary(&mut self, operator: &str, left: &Expr, right: &Expr) -> Result<Value, BcError> {
        if operator == "&&" {
            let left = self.eval(left)?;
            if !self.truthy(&left) {
                return Ok(Value::Number(Number::zero()));
            }
            let right = self.eval(right)?;
            return Ok(Value::Number(Number::from_i64(i64::from(
                self.truthy(&right),
            ))));
        }
        if operator == "||" {
            let left = self.eval(left)?;
            if self.truthy(&left) {
                return Ok(Value::Number(Number::one()));
            }
            let right = self.eval(right)?;
            return Ok(Value::Number(Number::from_i64(i64::from(
                self.truthy(&right),
            ))));
        }
        let left = self.eval(left)?;
        let right = self.eval(right)?;
        match operator {
            "+" | "-" | "*" | "/" | "%" | "^" => {
                let left = self.value_number(&left)?;
                let right = self.value_number(&right)?;
                let result = match operator {
                    "+" => left.add(&right),
                    "-" => left.sub(&right),
                    "*" => left.mul(&right, self.scale),
                    "/" => left
                        .div(&right, self.scale)
                        .ok_or_else(|| BcError::Runtime("division by zero".to_string()))?,
                    "%" => left
                        .rem(&right, self.scale)
                        .ok_or_else(|| BcError::Runtime("division by zero".to_string()))?,
                    _ => left
                        .pow(&right, self.scale)
                        .ok_or_else(|| BcError::Runtime("invalid exponent".to_string()))?,
                };
                Ok(Value::Number(result))
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                let ordering = match (&left, &right) {
                    (Value::Number(left), Value::Number(right)) => left.compare(right),
                    (Value::Str(left), Value::Str(right)) => left.cmp(right),
                    _ => {
                        return Err(BcError::Runtime(
                            "cannot compare a string and a number".to_string(),
                        ));
                    }
                };
                let result = match operator {
                    "==" => ordering == core::cmp::Ordering::Equal,
                    "!=" => ordering != core::cmp::Ordering::Equal,
                    "<" => ordering == core::cmp::Ordering::Less,
                    "<=" => ordering != core::cmp::Ordering::Greater,
                    ">" => ordering == core::cmp::Ordering::Greater,
                    _ => ordering != core::cmp::Ordering::Less,
                };
                Ok(Value::Number(Number::from_i64(i64::from(result))))
            }
            _ => Err(BcError::Runtime("unknown operator".to_string())),
        }
    }

    fn call(&mut self, name: &str, arguments: &[Expr]) -> Result<Value, BcError> {
        let mut values = Vec::new();
        for argument in arguments {
            values.push(self.eval(argument)?);
        }
        if let Some(function) = self.functions.get(name).cloned() {
            return Ok(Value::Number(self.call_user(&function, values)?));
        }
        match name {
            "sqrt" => {
                let value = self.single_number(name, &values)?;
                let root = value.sqrt(self.scale).ok_or_else(|| {
                    BcError::Runtime("square root of a negative number".to_string())
                })?;
                Ok(Value::Number(root))
            }
            "length" => {
                if values.len() != 1 {
                    return Err(BcError::Runtime("length takes one argument".to_string()));
                }
                let length = match &values[0] {
                    Value::Number(number) => number.length(),
                    Value::Str(text) => text.chars().count(),
                };
                Ok(Value::Number(Number::from_usize(length)))
            }
            "scale" => {
                if values.len() != 1 {
                    return Err(BcError::Runtime("scale takes one argument".to_string()));
                }
                let scale = match &values[0] {
                    Value::Number(number) => number.scale(),
                    Value::Str(_) => 0,
                };
                Ok(Value::Number(Number::from_usize(scale)))
            }
            "e" | "l" | "s" | "c" | "a" => {
                if !self.math_lib {
                    return Err(BcError::Unsupported(format!(
                        "math function `{name}` requires -l"
                    )));
                }
                let value = self.single_number(name, &values)?;
                let result = math::call(name, &value, self.scale).ok_or_else(|| {
                    BcError::Runtime(format!("{name} is outside the supported domain"))
                })?;
                Ok(Value::Number(result))
            }
            "j" => Err(BcError::Unsupported(
                "the Bessel function j is not implemented".to_string(),
            )),
            _ => Err(BcError::Runtime(format!("undefined function `{name}`"))),
        }
    }

    fn single_number(&mut self, name: &str, values: &[Value]) -> Result<Number, BcError> {
        if values.len() != 1 {
            return Err(BcError::Runtime(format!(
                "{name} takes exactly one argument"
            )));
        }
        self.value_number(&values[0])
    }

    fn call_user(&mut self, function: &Function, values: Vec<Value>) -> Result<Number, BcError> {
        if function.parameters.len() != values.len() {
            return Err(BcError::Runtime(format!(
                "{} expects {} arguments",
                function.name,
                function.parameters.len()
            )));
        }
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(BcError::Runtime(
                "function recursion limit exceeded".to_string(),
            ));
        }
        let mut frame = Frame {
            variables: BTreeMap::new(),
        };
        for (parameter, value) in function.parameters.iter().zip(values.iter()) {
            frame
                .variables
                .insert(parameter.clone(), self.value_number(value)?);
        }
        self.frames.push(frame);
        let result = self.exec_block(&function.body);
        self.frames.pop();
        match result? {
            Action::Return(value) => Ok(value),
            Action::None => Ok(Number::zero()),
            Action::Quit => {
                self.quit = true;
                Ok(Number::zero())
            }
            Action::Break | Action::Continue => Err(BcError::Runtime(
                "break or continue outside a loop".to_string(),
            )),
        }
    }

    fn exec_block(&mut self, statements: &[Stmt]) -> Result<Action, BcError> {
        for statement in statements {
            let action = self.exec_stmt(statement)?;
            if !matches!(action, Action::None) {
                return Ok(action);
            }
            if self.quit {
                return Ok(Action::Quit);
            }
        }
        Ok(Action::None)
    }

    fn truthy(&self, value: &Value) -> bool {
        match value {
            Value::Number(number) => !number.is_zero(),
            Value::Str(text) => !text.is_empty(),
        }
    }

    fn value_number(&mut self, value: &Value) -> Result<Number, BcError> {
        match value {
            Value::Number(number) => Ok(number.clone()),
            Value::Str(_) => Err(BcError::Runtime(
                "a string cannot be used as a number".to_string(),
            )),
        }
    }

    fn emit(&mut self, value: &Value, newline: bool) {
        match value {
            Value::Number(number) => {
                self.output.push_str(&number.to_decimal_string());
                if newline {
                    self.output.push('\n');
                }
            }
            Value::Str(text) => self.output.push_str(text),
        }
    }

    fn get_variable(&self, name: &str) -> Number {
        match name {
            "scale" => Number::from_usize(self.scale),
            "ibase" | "obase" => Number::from_i64(10),
            _ => {
                if let Some(frame) = self.frames.last()
                    && let Some(value) = frame.variables.get(name)
                {
                    return value.clone();
                }
                self.globals.get(name).cloned().unwrap_or_else(Number::zero)
            }
        }
    }

    fn assign(&mut self, target: &Expr, value: Value) -> Result<(), BcError> {
        match target {
            Expr::Var(name) => {
                if name == "scale" {
                    let scale = self
                        .value_number(&value)?
                        .to_i64()
                        .ok_or_else(|| BcError::Runtime("scale is out of range".to_string()))?;
                    let scale = scale.max(0) as usize;
                    if scale > MAX_SCALE {
                        return Err(BcError::Runtime("scale is too large".to_string()));
                    }
                    self.scale = scale;
                    return Ok(());
                }
                if name == "ibase" || name == "obase" {
                    let base = self.value_number(&value)?.to_i64().unwrap_or(0);
                    if base != 10 {
                        return Err(BcError::Unsupported(
                            "only decimal ibase/obase are implemented".to_string(),
                        ));
                    }
                    return Ok(());
                }
                let number = self.value_number(&value)?;
                if let Some(frame) = self.frames.last_mut()
                    && frame.variables.contains_key(name)
                {
                    frame.variables.insert(name.clone(), number);
                    return Ok(());
                }
                self.globals.insert(name.clone(), number);
                Ok(())
            }
            Expr::Array(name, index) => {
                let index = self.array_index(index)?;
                let number = self.value_number(&value)?;
                self.arrays
                    .entry(name.clone())
                    .or_default()
                    .insert(index, number);
                Ok(())
            }
            _ => Err(BcError::Runtime("assignment to a non-lvalue".to_string())),
        }
    }

    fn read_lvalue(&mut self, target: &Expr) -> Result<Value, BcError> {
        match target {
            Expr::Var(name) => Ok(Value::Number(self.get_variable(name))),
            Expr::Array(name, index) => {
                let index = self.array_index(index)?;
                Ok(Value::Number(self.get_array(name, index)))
            }
            _ => Err(BcError::Runtime("not an lvalue".to_string())),
        }
    }

    fn array_index(&mut self, index: &Expr) -> Result<i64, BcError> {
        let value = self.eval(index)?;
        let number = self.value_number(&value)?;
        let index = number
            .to_i64()
            .ok_or_else(|| BcError::Runtime("array index is out of range".to_string()))?;
        if !(0..=MAX_ARRAY_INDEX).contains(&index) {
            return Err(BcError::Runtime(
                "array index must be between 0 and 1000000".to_string(),
            ));
        }
        Ok(index)
    }

    fn get_array(&self, name: &str, index: i64) -> Number {
        self.arrays
            .get(name)
            .and_then(|values| values.get(&index))
            .cloned()
            .unwrap_or_else(Number::zero)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
