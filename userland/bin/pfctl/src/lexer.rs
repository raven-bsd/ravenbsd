// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! pf.conf tokenizer.
//!
//! Newlines are significant (they terminate statements); `#` starts a comment;
//! double-quoted strings keep spaces; every other run of non-delimiter bytes is
//! one word (addresses, numbers, port ranges and interface names arrive whole).
//! Line numbers are tracked for the parser's line-numbered errors.

use alloc::string::String;
use alloc::vec::Vec;

/// One lexical token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// An unquoted word (identifier, number, address, port range, `$macro`).
    Word(String),
    /// A double-quoted string, unescaped.
    Str(String),
    /// Statement terminator.
    Newline,
    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// `<`.
    Lt,
    /// `>`.
    Gt,
    /// `!`.
    Bang,
    /// `=`.
    Eq,
    /// `!=`.
    Ne,
    /// `<=`.
    Le,
    /// `>=`.
    Ge,
    /// `->`.
    Arrow,
    /// `,`.
    Comma,
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// End of input.
    Eof,
}

/// A token with its 1-based source line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned {
    /// The token.
    pub token: Token,
    /// 1-based line number.
    pub line: u32,
}

/// A lexer error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexError {
    /// 1-based line number.
    pub line: u32,
    /// Human-readable message.
    pub message: String,
}

/// Tokenize `input`.
pub fn tokenize(input: &str) -> Result<Vec<Spanned>, LexError> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut line = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\r' => index += 1,
            b'\n' => {
                tokens.push(Spanned {
                    token: Token::Newline,
                    line,
                });
                line += 1;
                index += 1;
            }
            b'#' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'"' => {
                let start_line = line;
                index += 1;
                let mut text = String::new();
                let mut closed = false;
                while index < bytes.len() {
                    match bytes[index] {
                        b'"' => {
                            closed = true;
                            index += 1;
                            break;
                        }
                        b'\\' if index + 1 < bytes.len() => {
                            let next = bytes[index + 1];
                            if next == b'"' || next == b'\\' {
                                text.push(char::from(next));
                                index += 2;
                            } else {
                                text.push('\\');
                                index += 1;
                            }
                        }
                        b'\n' => {
                            return Err(LexError {
                                line: start_line,
                                message: String::from("unterminated string"),
                            });
                        }
                        other => {
                            text.push(char::from(other));
                            index += 1;
                        }
                    }
                }
                if !closed {
                    return Err(LexError {
                        line: start_line,
                        message: String::from("unterminated string"),
                    });
                }
                tokens.push(Spanned {
                    token: Token::Str(text),
                    line: start_line,
                });
            }
            b'{' | b'}' | b',' | b'(' | b')' => {
                let token = match byte {
                    b'{' => Token::LBrace,
                    b'}' => Token::RBrace,
                    b',' => Token::Comma,
                    b'(' => Token::LParen,
                    _ => Token::RParen,
                };
                tokens.push(Spanned { token, line });
                index += 1;
            }
            b'!' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Spanned {
                        token: Token::Ne,
                        line,
                    });
                    index += 2;
                } else {
                    tokens.push(Spanned {
                        token: Token::Bang,
                        line,
                    });
                    index += 1;
                }
            }
            b'=' => {
                tokens.push(Spanned {
                    token: Token::Eq,
                    line,
                });
                index += 1;
            }
            b'<' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Spanned {
                        token: Token::Le,
                        line,
                    });
                    index += 2;
                } else {
                    tokens.push(Spanned {
                        token: Token::Lt,
                        line,
                    });
                    index += 1;
                }
            }
            b'>' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    tokens.push(Spanned {
                        token: Token::Ge,
                        line,
                    });
                    index += 2;
                } else {
                    tokens.push(Spanned {
                        token: Token::Gt,
                        line,
                    });
                    index += 1;
                }
            }
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                tokens.push(Spanned {
                    token: Token::Arrow,
                    line,
                });
                index += 2;
            }
            _ => {
                let start = index;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if matches!(
                        byte,
                        b' ' | b'\t'
                            | b'\r'
                            | b'\n'
                            | b'#'
                            | b'"'
                            | b'{'
                            | b'}'
                            | b'<'
                            | b'>'
                            | b','
                            | b'('
                            | b')'
                            | b'!'
                            | b'='
                    ) {
                        break;
                    }
                    index += 1;
                }
                if start == index {
                    return Err(LexError {
                        line,
                        message: alloc::format!("unexpected byte 0x{byte:02x}"),
                    });
                }
                let word = core::str::from_utf8(&bytes[start..index]).map_err(|_| LexError {
                    line,
                    message: String::from("input is not valid UTF-8"),
                })?;
                tokens.push(Spanned {
                    token: Token::Word(String::from(word)),
                    line,
                });
            }
        }
    }
    tokens.push(Spanned {
        token: Token::Eof,
        line,
    });
    Ok(tokens)
}

#[cfg(test)]
#[path = "lexer_ut.rs"]
mod tests;
