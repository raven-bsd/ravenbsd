// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the gettext plural expression evaluator from musl:src/locale/pleval.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! The GNU gettext plural-expression evaluator.

/// Maximum nesting depth of the donor's evaluator.
pub const PLURAL_DEPTH: i32 = 20;

/// Evaluate `expr` for `n`; `None` on a malformed expression or division by zero.
#[must_use]
pub fn plural_eval(expr: &[u8], n: u64) -> Option<u64> {
    let mut state = State { n };
    let mut parser = Parser {
        data: expr,
        pos: 0,
        depth: PLURAL_DEPTH,
    };
    let value = parser.expr(&mut state)?;
    parser.skip_space();
    if parser.peek() != Some(b';') && parser.peek().is_some() {
        return None;
    }
    if parser.peek() == Some(b';') {
        parser.pos += 1;
        parser.skip_space();
        if parser.peek().is_some() {
            return None;
        }
    }
    Some(value)
}

struct State {
    n: u64,
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    depth: i32,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn expr(&mut self, state: &mut State) -> Option<u64> {
        self.depth -= 1;
        if self.depth < 0 {
            return None;
        }
        self.skip_space();
        let condition = self.binary(state, 0)?;
        self.skip_space();
        if self.peek() != Some(b'?') {
            self.depth += 1;
            return Some(condition);
        }
        self.pos += 1;
        let left = self.expr(state)?;
        self.skip_space();
        if self.peek() != Some(b':') {
            return None;
        }
        self.pos += 1;
        let right = self.expr(state)?;
        self.depth += 1;
        Some(if condition != 0 { left } else { right })
    }

    fn binary(&mut self, state: &mut State, min_prec: u8) -> Option<u64> {
        let mut left = self.primary(state)?;
        loop {
            self.skip_space();
            let Some((op, len, precedence)) = self.operator() else {
                return Some(left);
            };
            if precedence <= min_prec {
                return Some(left);
            }
            self.pos += len;
            let right = self.binary(state, precedence)?;
            left = apply(op, left, right)?;
        }
    }

    fn operator(&self) -> Option<(u8, usize, u8)> {
        let first = self.peek()?;
        let second = self.data.get(self.pos + 1).copied();
        let pair = |a: u8, b: u8| (a, 2, b);
        match first {
            b'|' if second == Some(b'|') => Some(pair(0, 1)),
            b'&' if second == Some(b'&') => Some(pair(1, 2)),
            b'=' if second == Some(b'=') => Some(pair(2, 3)),
            b'!' if second == Some(b'=') => Some(pair(3, 3)),
            b'>' if second == Some(b'=') => Some(pair(4, 4)),
            b'<' if second == Some(b'=') => Some(pair(5, 4)),
            b'>' => Some((6, 1, 4)),
            b'<' => Some((7, 1, 4)),
            b'+' => Some((8, 1, 5)),
            b'-' => Some((9, 1, 5)),
            b'*' => Some((10, 1, 6)),
            b'/' => Some((12, 1, 6)),
            b'%' => Some((11, 1, 6)),
            _ => None,
        }
    }

    fn primary(&mut self, state: &mut State) -> Option<u64> {
        self.depth -= 1;
        if self.depth < 0 {
            return None;
        }
        self.skip_space();
        let result = match self.peek()? {
            b'(' => {
                self.pos += 1;
                let value = self.expr(state)?;
                self.skip_space();
                if self.peek() != Some(b')') {
                    return None;
                }
                self.pos += 1;
                value
            }
            b'!' => {
                self.pos += 1;
                let value = self.primary(state)?;
                u64::from(value == 0)
            }
            b'n' => {
                self.pos += 1;
                state.n
            }
            byte if byte.is_ascii_digit() => {
                let mut value = 0u64;
                while let Some(byte) = self.peek() {
                    if !byte.is_ascii_digit() {
                        break;
                    }
                    value = value.wrapping_mul(10).wrapping_add(u64::from(byte - b'0'));
                    self.pos += 1;
                }
                value
            }
            _ => return None,
        };
        self.depth += 1;
        Some(result)
    }
}

fn apply(op: u8, left: u64, right: u64) -> Option<u64> {
    Some(match op {
        0 => u64::from(left != 0 || right != 0),
        1 => u64::from(left != 0 && right != 0),
        2 => u64::from(left == right),
        3 => u64::from(left != right),
        4 => u64::from(left >= right),
        5 => u64::from(left <= right),
        6 => u64::from(left > right),
        7 => u64::from(left < right),
        8 => left.wrapping_add(right),
        9 => left.wrapping_sub(right),
        10 => left.wrapping_mul(right),
        11 => {
            if right == 0 {
                return None;
            }
            left % right
        }
        12 => {
            if right == 0 {
                return None;
            }
            left / right
        }
        _ => return None,
    })
}

#[cfg(test)]
#[path = "plural_ut.rs"]
mod tests;
