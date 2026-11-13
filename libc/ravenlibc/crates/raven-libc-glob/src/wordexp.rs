// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/misc/wordexp.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header upstream; covered by musl/COPYRIGHT, H-1)
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `wordexp` and `wordfree`'s data, using the musl shell-script design (doc 07 4.6).
//!
//! The `WRDE_NOCMD` pre-scan is the donor's `switch` over the input (`wordexp.c:41-84`): it
//! rejects an unquoted newline, `|`, `&`, `;`, `<`, `>`, `{`, `}` and an unbalanced parenthesis
//! with [`WRDE_BADCHAR`], and backquote or `$(` with [`WRDE_CMDSUB`], before the shell is run.
//! The shell is invoked through [`ShellRun`] with the donor's script
//! ([`WORDEXP_SCRIPT`]); the fork/pipe/wait code of the donor is
//! the process crate's. The first NUL-terminated word (`x`) is the sentinel and is dropped.
//!
//! `WRDE_UNDEF` is accepted but not enforced in-process: the shell expands an undefined variable
//! to the empty string. This is the documented musl-vs-FreeBSD difference (doc 07 7.3).

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use crate::consts::{
    WORDEXP_SCRIPT, WRDE_APPEND, WRDE_BADCHAR, WRDE_CMDSUB, WRDE_DOOFFS, WRDE_NOSPACE, WRDE_REUSE,
    WRDE_SHOWERR, WRDE_SYNTAX,
};

/// Runs `/bin/sh -c <script> sh <arg>` and returns its standard output.
///
/// The process crate implements this with `posix_spawn`, a full signal mask and a pipe (C53/C64);
/// `show_stderr` false means the child's standard error is `/dev/null`.
pub trait ShellRun {
    /// Run the script with `arg` as `$1`; return the bytes written to standard output.
    ///
    /// # Errors
    ///
    /// The process backend's error (`ENOMEM`, `EMFILE`, `ENOENT`, ...).
    fn run(&mut self, script: &[u8], arg: &[u8], show_stderr: bool) -> Result<Vec<u8>, Errno>;
}

/// The `wordexp_t` result: the expanded words and the `we_offs` count.
#[derive(Clone, Debug, Default)]
pub struct WordExp {
    /// The expanded words (the shell's output without the sentinel).
    pub words: Vec<Vec<u8>>,
    /// The `we_offs` count the shell reserves in `we_wordv`.
    pub offs: usize,
}

/// The byte at `index`, or NUL.
fn atb(s: &[u8], index: usize) -> u8 {
    s.get(index).copied().unwrap_or(0)
}

/// The `WRDE_NOCMD` pre-scan; returns `0` or one of `WRDE_BADCHAR`/`WRDE_CMDSUB`/`WRDE_SYNTAX`.
#[must_use]
pub fn prescan(s: &[u8]) -> i32 {
    let mut i = 0usize;
    let mut sq = false;
    let mut dq = false;
    let mut np = 0usize;
    while i < s.len() {
        match atb(s, i) {
            b'\\' => {
                if !sq {
                    i = i.wrapping_add(1);
                    if i >= s.len() {
                        return WRDE_SYNTAX;
                    }
                }
            }
            b'\'' => {
                if !dq {
                    sq = !sq;
                }
            }
            b'"' => {
                if !sq {
                    dq = !dq;
                }
            }
            b'(' => {
                if np > 0 {
                    np = np.wrapping_add(1);
                } else if !(sq || dq) {
                    return WRDE_BADCHAR;
                }
            }
            b')' => {
                if np > 0 {
                    np = np.wrapping_sub(1);
                } else if !(sq || dq) {
                    return WRDE_BADCHAR;
                }
            }
            b'\n' | b'|' | b'&' | b';' | b'<' | b'>' | b'{' | b'}' => {
                if !(sq || dq || np > 0) {
                    return WRDE_BADCHAR;
                }
            }
            b'$' => {
                if !sq {
                    if atb(s, i.wrapping_add(1)) == b'(' && atb(s, i.wrapping_add(2)) == b'(' {
                        i = i.wrapping_add(2);
                        np = np.wrapping_add(2);
                    } else if atb(s, i.wrapping_add(1)) == b'(' {
                        return WRDE_CMDSUB;
                    }
                }
            }
            b'`' if !sq => return WRDE_CMDSUB,
            _ => {}
        }
        i = i.wrapping_add(1);
    }
    0
}

/// Split the shell's NUL-separated output, dropping the `x` sentinel.
fn parse_output(output: &[u8]) -> Result<Vec<Vec<u8>>, i32> {
    let Some(sentinel) = output.iter().position(|&b| b == 0) else {
        return Err(WRDE_SYNTAX);
    };
    let tail = output.get(sentinel.wrapping_add(1)..).unwrap_or(&[]);
    let mut words = Vec::new();
    if tail.is_empty() {
        return Ok(words);
    }
    let ends_with_nul = tail.last() == Some(&0);
    let mut start = 0usize;
    for (index, &byte) in tail.iter().enumerate() {
        if byte != 0 {
            continue;
        }
        words.push(tail.get(start..index).unwrap_or(&[]).to_vec());
        start = index.wrapping_add(1);
    }
    if !ends_with_nul {
        words.push(tail.get(start..).unwrap_or(&[]).to_vec());
    }
    Ok(words)
}

/// POSIX `wordexp`: expand `s` into [`WordExp::words`], returning `0` or a `WRDE_*` code.
///
/// `WRDE_APPEND` extends the previous words, `WRDE_REUSE` frees them first, `WRDE_DOOFFS`
/// validates `out.offs`, and `WRDE_SHOWERR` is passed to the shell runner.
pub fn wordexp(run: &mut dyn ShellRun, s: &[u8], flags: u32, out: &mut WordExp) -> i32 {
    if flags & WRDE_REUSE != 0 || flags & WRDE_APPEND == 0 {
        out.words.clear();
    }
    if flags & WRDE_DOOFFS == 0 {
        out.offs = 0;
    }
    if flags & WRDE_DOOFFS != 0 && out.offs > usize::MAX / 32 {
        return WRDE_NOSPACE;
    }
    if flags & crate::consts::WRDE_NOCMD != 0 {
        let rv = prescan(s);
        if rv != 0 {
            return rv;
        }
    }
    let output = match run.run(WORDEXP_SCRIPT, s, flags & WRDE_SHOWERR != 0) {
        Ok(output) => output,
        Err(_) => return WRDE_NOSPACE,
    };
    match parse_output(&output) {
        Ok(words) => {
            out.words.extend(words);
            0
        }
        Err(code) => code,
    }
}

#[cfg(test)]
#[path = "wordexp_ut.rs"]
mod tests;
