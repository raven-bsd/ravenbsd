// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Flag values, error codes and the fixed limits of the Spencer engine.
//!
//! The values are facts read from the BSD `<regex.h>` headers (NetBSD, FreeBSD and OpenBSD
//! agree on every value; no header text is copied). The hardening limits are RavenBSD
//! additions; the donor has no equivalent.

/// No flag: the pattern is a BRE (NetBSD `REG_BASIC`).
pub const REG_BASIC: u32 = 0o0;
/// The pattern is an ERE.
pub const REG_EXTENDED: u32 = 0o1;
/// Case-insensitive matching.
pub const REG_ICASE: u32 = 0o2;
/// Do not report subexpressions.
pub const REG_NOSUB: u32 = 0o4;
/// `^` and `$` also match around newlines; `.` and inverted sets exclude newline.
pub const REG_NEWLINE: u32 = 0o10;
/// The pattern has no metacharacters.
pub const REG_NOSPEC: u32 = 0o20;
/// The pattern end is given explicitly (the shell expresses it by the slice length).
pub const REG_PEND: u32 = 0o40;
/// Debug dump of the strip (`REG_DUMP`, accepted and ignored).
pub const REG_DUMP: u32 = 0o200;
/// GNU extensions (`\w`, `\s`, `\b`, `\<`-style anchors and ERE back-references).
pub const REG_GNU: u32 = 0o400;

/// `^` does not match at the start of the subject.
pub const REG_NOTBOL: u32 = 0o1;
/// `$` does not match at the end of the subject.
pub const REG_NOTEOL: u32 = 0o2;
/// `pmatch[0]` carries the input window `[rm_so, rm_eo)` in, and the match out.
pub const REG_STARTEND: u32 = 0o4;
/// Execution tracing (`REG_TRACE`, accepted and ignored).
pub const REG_TRACE: u32 = 0o400;
/// Force the large (byte-array) state representation.
pub const REG_LARGE: u32 = 0o1000;
/// Force the back-reference walker even when the pattern has no `\N`.
pub const REG_BACKR: u32 = 0o2000;

/// `regexec` did not match.
pub const REG_NOMATCH: i32 = 1;
/// Invalid regular expression.
pub const REG_BADPAT: i32 = 2;
/// Invalid collating element.
pub const REG_ECOLLATE: i32 = 3;
/// Invalid character class.
pub const REG_ECTYPE: i32 = 4;
/// Trailing backslash.
pub const REG_EESCAPE: i32 = 5;
/// Invalid back-reference number.
pub const REG_ESUBREG: i32 = 6;
/// Brackets are not balanced.
pub const REG_EBRACK: i32 = 7;
/// Parentheses are not balanced.
pub const REG_EPAREN: i32 = 8;
/// Braces are not balanced.
pub const REG_EBRACE: i32 = 9;
/// Invalid repetition count.
pub const REG_BADBR: i32 = 10;
/// Invalid character range.
pub const REG_ERANGE: i32 = 11;
/// Out of memory, or one of the hardening limits was exceeded.
pub const REG_ESPACE: i32 = 12;
/// Repetition operator with an invalid operand.
pub const REG_BADRPT: i32 = 13;
/// Empty (sub)expression.
pub const REG_EMPTY: i32 = 14;
/// Internal assertion ("can't happen").
pub const REG_ASSERT: i32 = 15;
/// Invalid argument to a regex routine.
pub const REG_INVARG: i32 = 16;
/// Illegal byte sequence.
pub const REG_ILLSEQ: i32 = 17;

/// Convert an error name to its number (`REG_ATOI`).
pub const REG_ATOI: i32 = 255;
/// Convert an error number to its name (`REG_ITOA`).
pub const REG_ITOA: i32 = 0o400;

/// Number of parenthesised groups whose span is remembered for back-references.
pub const NPAREN: usize = 10;
/// `RE_DUP_MAX`: the largest repetition count accepted by `p_count`.
pub const DUPMAX: i32 = 255;
/// The "no upper bound" marker of `{m,}`; one more than [`DUPMAX`].
pub const INFINITY: i32 = DUPMAX + 1;
/// Maximum recursion depth of the backtracking back-reference walker.
pub const MAX_RECURSION: i32 = 100;

/// Hardening H-RE-1: the largest strip (in `sop` words) the compiler will allocate.
pub const MAX_STRIP: usize = 1 << 21;
/// Hardening H-RE-2: `backref` calls allowed per `regexec`.
pub const BACKREF_BUDGET: u64 = 1 << 24;
/// Hardening H-RE-3: maximum nested parenthesised groups in the parser.
pub const MAX_PAREN_DEPTH: usize = 256;
/// Maximum nested `backref` activation frames before `REG_ESPACE` (see the report).
pub const BACKREF_DEPTH_MAX: u32 = 4096;

/// Internal flag: the pattern uses `^`.
pub const USEBOL: i32 = 0o1;
/// Internal flag: the pattern uses `$`.
pub const USEEOL: i32 = 0o2;
/// Internal flag: the compiled strip is inconsistent.
pub const BAD: i32 = 0o4;

/// Outer magic word of a valid `Regex`.
pub const MAGIC1: i32 = ((b'r' as i32 ^ 0o200) << 8) | b'e' as i32;
/// Inner magic word of a valid `re_guts`.
pub const MAGIC2: i32 = ((b'R' as i32 ^ 0o200) << 8) | b'E' as i32;

/// Shift of the operator field in a strip word.
pub const OPSHIFT: u32 = 27;
/// Mask of the operator field in a strip word.
pub const OPRMASK: u32 = 0xf800_0000;
/// Mask of the operand field in a strip word.
pub const OPDMASK: u32 = 0x07ff_ffff;

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
