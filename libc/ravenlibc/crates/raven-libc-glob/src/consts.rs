// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! FreeBSD flag values, error codes and expansion limits for `fnmatch`, `glob` and `wordexp`.
//!
//! The numbers are facts read from `freebsd:include/{fnmatch,glob,wordexp}.h` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`; musl's different flag *values* are not used
//! (A-glob-3). The `glob` limits are the donor's `freebsd:lib/libc/gen/glob.c:101-105`.

/// `FNM_NOESCAPE`: `\` is an ordinary character.
pub const FNM_NOESCAPE: u32 = 0x01;
/// `FNM_PATHNAME`: a slash in the string is matched only by a slash in the pattern.
pub const FNM_PATHNAME: u32 = 0x02;
/// `FNM_PERIOD`: a leading period is matched only literally.
pub const FNM_PERIOD: u32 = 0x04;
/// `FNM_LEADING_DIR`: ignore `/<tail>` after a match.
pub const FNM_LEADING_DIR: u32 = 0x08;
/// `FNM_CASEFOLD`: case-insensitive comparison.
pub const FNM_CASEFOLD: u32 = 0x10;
/// `FNM_IGNORECASE`: alias of [`FNM_CASEFOLD`].
pub const FNM_IGNORECASE: u32 = FNM_CASEFOLD;
/// `FNM_FILE_NAME`: alias of [`FNM_PATHNAME`].
pub const FNM_FILE_NAME: u32 = FNM_PATHNAME;
/// The only failure value `fnmatch` returns (`FNM_NOMATCH`).
pub const FNM_NOMATCH: i32 = 1;
/// `FNM_NOSYS`: reserved, never returned by this implementation.
pub const FNM_NOSYS: i32 = -1;

/// `GLOB_APPEND`: append to a previous call's results.
pub const GLOB_APPEND: u32 = 0x0001;
/// `GLOB_DOOFFS`: `gl_offs` leading null entries are reserved.
pub const GLOB_DOOFFS: u32 = 0x0002;
/// `GLOB_ERR`: abort on a directory error.
pub const GLOB_ERR: u32 = 0x0004;
/// `GLOB_MARK`: append `/` to directory results.
pub const GLOB_MARK: u32 = 0x0008;
/// `GLOB_NOCHECK`: return the pattern when nothing matches.
pub const GLOB_NOCHECK: u32 = 0x0010;
/// `GLOB_NOSORT`: keep directory order.
pub const GLOB_NOSORT: u32 = 0x0020;
/// `GLOB_ALTDIRFUNC`: the caller supplies the directory access functions.
pub const GLOB_ALTDIRFUNC: u32 = 0x0040;
/// `GLOB_BRACE`: expand csh-style `{a,b}` lists.
pub const GLOB_BRACE: u32 = 0x0080;
/// `GLOB_MAGCHAR`: the pattern contained a metacharacter (output flag).
pub const GLOB_MAGCHAR: u32 = 0x0100;
/// `GLOB_NOMAGIC`: `GLOB_NOCHECK` only when the pattern has no metacharacters.
pub const GLOB_NOMAGIC: u32 = 0x0200;
/// `GLOB_QUOTE`: `\` quotes the next character.
pub const GLOB_QUOTE: u32 = 0x0400;
/// `GLOB_TILDE`: expand a leading `~`.
pub const GLOB_TILDE: u32 = 0x0800;
/// `GLOB_LIMIT`: enforce the expansion limits of the donor.
pub const GLOB_LIMIT: u32 = 0x1000;
/// `GLOB_NOESCAPE`: disable backslash escaping.
pub const GLOB_NOESCAPE: u32 = 0x2000;
/// `GLOB_MAXPATH`: source-compatible alias of [`GLOB_LIMIT`].
pub const GLOB_MAXPATH: u32 = GLOB_LIMIT;
/// `GLOB_ABEND`: source-compatible alias of `GLOB_ABORTED`.
pub const GLOB_ABEND: i32 = GLOB_ABORTED;

/// No space (or a limit was exceeded); `errno` carries `E2BIG` for a limit.
pub const GLOB_NOSPACE: i32 = -1;
/// A directory error was not ignored.
pub const GLOB_ABORTED: i32 = -2;
/// Nothing matched and `GLOB_NOCHECK` was not set.
pub const GLOB_NOMATCH: i32 = -3;
/// Obsolete source-compatibility value.
pub const GLOB_NOSYS: i32 = -4;

/// Brace expansions per call (`GLOB_LIMIT_BRACE`).
pub const GLOB_LIMIT_BRACE: usize = 128;
/// Path elements per call when `gl_matchc` is zero (`GLOB_LIMIT_PATH`).
pub const GLOB_LIMIT_PATH: usize = 65536;
/// Directory entries read per call (`GLOB_LIMIT_READDIR`).
pub const GLOB_LIMIT_READDIR: usize = 16384;
/// `stat` calls per call (`GLOB_LIMIT_STAT`).
pub const GLOB_LIMIT_STAT: usize = 1024;
/// Total bytes of returned paths per call (`GLOB_LIMIT_STRING`, `ARG_MAX`, H-19).
pub const GLOB_LIMIT_STRING: usize = 131072;
/// The donor's `MAXPATHLEN`: the pattern and path buffer bound.
pub const MAXPATHLEN: usize = 4096;

/// `WRDE_APPEND`: append to a previous call's words.
pub const WRDE_APPEND: u32 = 0x1;
/// `WRDE_DOOFFS`: `we_offs` leading null entries are reserved.
pub const WRDE_DOOFFS: u32 = 0x2;
/// `WRDE_NOCMD`: reject command substitution before running the shell.
pub const WRDE_NOCMD: u32 = 0x4;
/// `WRDE_REUSE`: free the previous result first.
pub const WRDE_REUSE: u32 = 0x8;
/// `WRDE_SHOWERR`: do not redirect the shell's standard error.
pub const WRDE_SHOWERR: u32 = 0x10;
/// `WRDE_UNDEF`: request an error for undefined variables (accepted; see the README).
pub const WRDE_UNDEF: u32 = 0x20;

/// An unquoted special character.
pub const WRDE_BADCHAR: i32 = 1;
/// The shell reported an undefined variable (`WRDE_UNDEF`; see the README).
pub const WRDE_BADVAL: i32 = 2;
/// Command substitution while `WRDE_NOCMD` is set.
pub const WRDE_CMDSUB: i32 = 3;
/// Out of memory or the shell could not be run.
pub const WRDE_NOSPACE: i32 = 4;
/// Obsolete, reserved.
pub const WRDE_NOSYS: i32 = 5;
/// A shell syntax error.
pub const WRDE_SYNTAX: i32 = 6;

/// The script `wordexp` passes to `/bin/sh -c`: `eval "set --;printf %s\\\\0 x $1"`.
///
/// `%s\\\\0` in this Rust literal is the four-byte sequence `\\\\` seen by the shell, which
/// becomes `\\0` for `printf` after `eval`, i.e. a NUL after every word (musl `wordexp.c:118-121`).
pub const WORDEXP_SCRIPT: &[u8] = b"eval \"set --;printf %s\\\\\\\\0 x $1\"";

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
