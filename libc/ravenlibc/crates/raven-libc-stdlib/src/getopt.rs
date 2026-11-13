// SPDX-License-Identifier: ISC AND BSD-2-Clause
// Copyright (c) 2002 Todd C. Miller <millert@openbsd.org>
// Copyright (c) 2000 The NetBSD Foundation, Inc.
// All rights reserved.
// This code is derived from software contributed to The NetBSD Foundation
// by Dieter Baron and Thomas Klausner.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/getopt_long.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tags: $OpenBSD: getopt_long.c,v 1.32 2020/05/27 22:25:09 schwarze Exp $,
// $NetBSD: getopt_long.c,v 1.15 2002/01/31 22:43:40 tv Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::too_many_arguments
)] // RavenBSD: fixed ticket signatures and donor argv indices

//! `getopt`, `getopt_long` and `getopt_long_only`.
//!
//! Decision D-3 / RD-25: plain [`GetoptState::getopt`] does **not** permute (`GETOPT_PERMUTES`
//! is `false`); `getopt_long` and `getopt_long_only` do. The donor's function-local statics
//! become fields of [`GetoptState`], `getenv("POSIXLY_CORRECT")` becomes a parameter, and the
//! `flag` pointer is reported as an index for the shell to resolve (`flag_to_set`).

/// Whether plain `getopt` passes `FLAG_PERMUTE` (O-8).
pub const GETOPT_PERMUTES: bool = false;

/// `no_argument`.
pub const NO_ARGUMENT: i32 = 0;
/// `required_argument`.
pub const REQUIRED_ARGUMENT: i32 = 1;
/// `optional_argument`.
pub const OPTIONAL_ARGUMENT: i32 = 2;

const FLAG_PERMUTE: u32 = 0x01;
const FLAG_ALLARGS: u32 = 0x02;
const FLAG_LONGONLY: u32 = 0x04;

const BADCH: i32 = b'?' as i32;
const INORDER: i32 = 1;

/// The C `struct option`, with the `flag` pointer reduced to its nullness.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Option_ {
    /// The long name, without the leading `--`.
    pub name: &'static [u8],
    /// [`NO_ARGUMENT`], [`REQUIRED_ARGUMENT`] or [`OPTIONAL_ARGUMENT`].
    pub has_arg: i32,
    /// Whether the C `flag` member is `NULL`.
    pub flag_is_null: bool,
    /// The value returned (or stored through `flag`).
    pub val: i32,
}

/// A long option over an arbitrary lifetime (the shell's table).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct LongOpt<'a> {
    /// The long name, without the leading `--`.
    pub name: &'a [u8],
    /// [`NO_ARGUMENT`], [`REQUIRED_ARGUMENT`] or [`OPTIONAL_ARGUMENT`].
    pub has_arg: i32,
    /// Whether the C `flag` member is `NULL`.
    pub flag_is_null: bool,
    /// The value returned (or stored through `flag`).
    pub val: i32,
}

/// Where the option argument lives.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum OptArg {
    /// No argument.
    None,
    /// The text of `argv[arg_index]` from `offset`.
    At {
        /// Current index into the (possibly permuted) argv.
        arg_index: usize,
        /// Byte offset of the argument within that string.
        offset: usize,
    },
}

/// Why an argument source is needed: the engine never owns the strings.
pub trait ArgSource {
    /// The argv token type (a pointer in the shell, a `usize` in tests).
    type Arg: Copy;

    /// The text of one argument.
    fn text(&self, a: Self::Arg) -> &[u8];
}

/// The error printer; the shell prepends the program name and appends a newline.
pub trait Diag {
    /// Print the concatenation of `parts` as one `warnx` line.
    fn warnx(&mut self, parts: &[&[u8]]);
}

/// One `getopt` result.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Step {
    /// Option character, `-1`, `'?'`, `':'`, `1` (for `-` prefix) or the long option value.
    pub code: i32,
    /// The option argument.
    pub optarg: OptArg,
    /// The matched long-option index, when a long option matched.
    pub long_index: Option<usize>,
    /// The long-option index whose `flag` the shell must set.
    pub flag_to_set: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
enum Place {
    Empty,
    At { index: usize, offset: usize },
}

/// The parser state the shell keeps in its global object.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct GetoptState {
    optind: i32,
    opterr: i32,
    optopt: i32,
    optreset: bool,
    place: Place,
    nonopt_start: i32,
    nonopt_end: i32,
    posixly_correct: Option<bool>,
}

impl GetoptState {
    /// `optind = 1`, `opterr = 1`, no current place.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            optind: 1,
            opterr: 1,
            optopt: b'?' as i32,
            optreset: false,
            place: Place::Empty,
            nonopt_start: -1,
            nonopt_end: -1,
            posixly_correct: None,
        }
    }

    /// The current `optind`.
    #[must_use]
    pub const fn optind(&self) -> i32 {
        self.optind
    }

    /// The current `optopt`.
    #[must_use]
    pub const fn optopt(&self) -> i32 {
        self.optopt
    }

    /// Set `optind`.
    pub fn set_optind(&mut self, v: i32) {
        self.optind = v;
    }

    /// Set `opterr`.
    pub fn set_opterr(&mut self, v: i32) {
        self.opterr = v;
    }

    /// Set `optreset`.
    pub fn set_optreset(&mut self, v: bool) {
        self.optreset = v;
    }

    /// Parse `argv` with the BSD (non-permuting) `getopt`.
    pub fn getopt<S: ArgSource, D: Diag>(
        &mut self,
        argv: &mut [S::Arg],
        src: &S,
        optstring: &[u8],
        posixly_correct: bool,
        d: &mut D,
    ) -> Step {
        let flags = if GETOPT_PERMUTES { FLAG_PERMUTE } else { 0 };
        self.internal(argv, src, optstring, None, flags, posixly_correct, d)
    }

    /// Parse `argv` with `getopt_long` (`only == false`) or `getopt_long_only` (`only == true`).
    pub fn getopt_long<S: ArgSource, D: Diag>(
        &mut self,
        argv: &mut [S::Arg],
        src: &S,
        optstring: &[u8],
        longopts: &[LongOpt],
        only: bool,
        posixly_correct: bool,
        d: &mut D,
    ) -> Step {
        let mut flags = FLAG_PERMUTE;
        if only {
            flags |= FLAG_LONGONLY;
        }
        self.internal(
            argv,
            src,
            optstring,
            Some(longopts),
            flags,
            posixly_correct,
            d,
        )
    }

    /// `getopt_long` without the `only` switch.
    pub fn getopt_long_only<S: ArgSource, D: Diag>(
        &mut self,
        argv: &mut [S::Arg],
        src: &S,
        optstring: &[u8],
        longopts: &[LongOpt],
        posixly_correct: bool,
        d: &mut D,
    ) -> Step {
        self.getopt_long(argv, src, optstring, longopts, true, posixly_correct, d)
    }

    fn internal<S: ArgSource, D: Diag>(
        &mut self,
        argv: &mut [S::Arg],
        src: &S,
        optstring: &[u8],
        longopts: Option<&[LongOpt]>,
        mut flags: u32,
        posixly_correct: bool,
        d: &mut D,
    ) -> Step {
        let mut options = optstring;

        if self.optind == 0 {
            self.optind = 1;
            self.optreset = true;
        }
        if self.posixly_correct.is_none() || self.optreset {
            self.posixly_correct = Some(posixly_correct);
        }
        let cached_posix = self.posixly_correct.unwrap_or(posixly_correct);
        if options.first() == Some(&b'-') {
            flags |= FLAG_ALLARGS;
        } else if cached_posix || options.first() == Some(&b'+') {
            flags &= !FLAG_PERMUTE;
        }
        if options.first() == Some(&b'+') || options.first() == Some(&b'-') {
            options = options.get(1..).unwrap_or(&[]);
        }
        if self.optreset {
            self.nonopt_start = -1;
            self.nonopt_end = -1;
        }

        'start: loop {
            if self.optreset || self.place == Place::Empty || self.place_at_end(argv, src) {
                self.optreset = false;
                if self.optind >= len_i32(argv) {
                    self.place = Place::Empty;
                    if self.nonopt_end != -1 {
                        self.permute_args(self.nonopt_start, self.nonopt_end, self.optind, argv);
                        self.optind -= self.nonopt_end - self.nonopt_start;
                    } else if self.nonopt_start != -1 {
                        self.optind = self.nonopt_start;
                    }
                    self.nonopt_start = -1;
                    self.nonopt_end = -1;
                    return step(-1);
                }
                let index = self.optind as usize;
                let text = src.text(argv[index]);
                let first = text.first().copied().unwrap_or(0);
                let single_dash = text.len() == 1;
                let dash_is_option = options.contains(&b'-');
                if first != b'-' || (single_dash && !dash_is_option) {
                    self.place = Place::Empty;
                    if flags & FLAG_ALLARGS != 0 {
                        let arg_index = index;
                        self.optind += 1;
                        return step_arg(INORDER, arg_index, 0);
                    }
                    if flags & FLAG_PERMUTE == 0 {
                        return step(-1);
                    }
                    if self.nonopt_start == -1 {
                        self.nonopt_start = self.optind;
                    } else if self.nonopt_end != -1 {
                        self.permute_args(self.nonopt_start, self.nonopt_end, self.optind, argv);
                        self.nonopt_start = self.optind - (self.nonopt_end - self.nonopt_start);
                        self.nonopt_end = -1;
                    }
                    self.optind += 1;
                    continue 'start;
                }
                if self.nonopt_start != -1 && self.nonopt_end == -1 {
                    self.nonopt_end = self.optind;
                }
                self.place = Place::At { index, offset: 0 };
                if !single_dash {
                    let second = text.get(1).copied().unwrap_or(0);
                    if second == b'-' && text.len() == 2 {
                        self.optind += 1;
                        self.place = Place::Empty;
                        if self.nonopt_end != -1 {
                            self.permute_args(
                                self.nonopt_start,
                                self.nonopt_end,
                                self.optind,
                                argv,
                            );
                            self.optind -= self.nonopt_end - self.nonopt_start;
                        }
                        self.nonopt_start = -1;
                        self.nonopt_end = -1;
                        return step(-1);
                    }
                    self.place = Place::At { index, offset: 1 };
                }
            }

            if let Some(longopts) = longopts {
                let at_start = matches!(
                    self.place,
                    Place::At { index, offset } if offset == 0 && index as i32 == self.optind
                );
                let here = self.place_byte(argv, src);
                if !at_start && (here == b'-' || flags & FLAG_LONGONLY != 0) {
                    let mut short_too = false;
                    if here == b'-' {
                        self.advance_place(argv, src);
                    } else if here != b':' && options.contains(&here) {
                        short_too = true;
                    }
                    let result =
                        self.parse_long_options(argv, src, options, longopts, short_too, flags, d);
                    match result {
                        Some(found) if found.code != -1 => {
                            self.place = Place::Empty;
                            return found;
                        }
                        _ => {}
                    }
                }
            }

            let optchar = i32::from(self.place_byte(argv, src));
            self.advance_place(argv, src);
            let Some(option_index) = options.iter().position(|byte| i32::from(*byte) == optchar)
            else {
                if self.place_at_end(argv, src) {
                    self.optind += 1;
                }
                self.print_error(options, d, &[b"unknown option -- ", &[optchar as u8]]);
                self.optopt = optchar;
                return step(BADCH);
            };
            if longopts.is_some() && optchar == i32::from(b'W') {
                let next = options.get(option_index + 1).copied().unwrap_or(0);
                if next == b';' {
                    if self.place_at_end(argv, src) {
                        self.optind += 1;
                        if self.optind >= len_i32(argv) {
                            self.place = Place::Empty;
                            self.print_error(
                                options,
                                d,
                                &[b"option requires an argument -- ", b"W"],
                            );
                            self.optopt = optchar;
                            return step(badarg(options));
                        }
                        let index = self.optind as usize;
                        self.place = Place::At { index, offset: 0 };
                    }
                    let longopts = longopts.unwrap_or(&[]);
                    let found =
                        self.parse_long_options(argv, src, options, longopts, false, flags, d);
                    self.place = Place::Empty;
                    return found.unwrap_or_else(|| step(-1));
                }
            }
            let next = options.get(option_index + 1).copied().unwrap_or(0);
            if next != b':' {
                if self.place_at_end(argv, src) {
                    self.optind += 1;
                }
            } else {
                let mut optarg = OptArg::None;
                if let Place::At { index, offset } = self.place {
                    let text = src.text(argv[index]);
                    if offset < text.len() {
                        optarg = OptArg::At {
                            arg_index: index,
                            offset,
                        };
                    } else {
                        let optional = options.get(option_index + 2).copied() == Some(b':');
                        if !optional {
                            self.optind += 1;
                            if self.optind >= len_i32(argv) {
                                self.place = Place::Empty;
                                self.print_error(
                                    options,
                                    d,
                                    &[b"option requires an argument -- ", &[optchar as u8]],
                                );
                                self.optopt = optchar;
                                return step(badarg(options));
                            }
                            optarg = OptArg::At {
                                arg_index: self.optind as usize,
                                offset: 0,
                            };
                        }
                    }
                }
                self.place = Place::Empty;
                self.optind += 1;
                return Step {
                    code: optchar,
                    optarg,
                    long_index: None,
                    flag_to_set: None,
                };
            }
            return Step {
                code: optchar,
                optarg: OptArg::None,
                long_index: None,
                flag_to_set: None,
            };
        }
    }

    fn parse_long_options<S: ArgSource, D: Diag>(
        &mut self,
        argv: &mut [S::Arg],
        src: &S,
        options: &[u8],
        longopts: &[LongOpt],
        short_too: bool,
        flags: u32,
        d: &mut D,
    ) -> Option<Step> {
        let (place_index, place_offset) = match self.place {
            Place::At { index, offset } => (index, offset),
            Place::Empty => (0usize, 0usize),
        };
        let text = src.text(*argv.get(place_index)?);
        let current = text.get(place_offset..).unwrap_or(&[]);
        let equal = current.iter().position(|byte| *byte == b'=');
        let (name, value_offset) = match equal {
            Some(position) => (
                current.get(..position).unwrap_or(&[]),
                place_offset + position + 1,
            ),
            None => (current, 0usize),
        };
        let mut match_index: Option<usize> = None;
        let mut exact_match = false;
        let mut second_partial_match = false;

        self.optind += 1;

        let mut index = 0usize;
        while index < longopts.len() {
            let Some(option) = longopts.get(index) else {
                break;
            };
            let prefix_matches =
                option.name.len() >= name.len() && option.name.get(..name.len()) == Some(name);
            if !prefix_matches {
                index += 1;
                continue;
            }
            if option.name.len() == name.len() {
                match_index = Some(index);
                exact_match = true;
                break;
            }
            if short_too && name.len() == 1 {
                index += 1;
                continue;
            }
            match match_index {
                None => match_index = Some(index),
                Some(previous) => {
                    let previous = match longopts.get(previous) {
                        Some(previous) => *previous,
                        None => *option,
                    };
                    if flags & FLAG_LONGONLY != 0
                        || option.has_arg != previous.has_arg
                        || option.flag_is_null != previous.flag_is_null
                        || option.val != previous.val
                    {
                        second_partial_match = true;
                    }
                }
            }
            index += 1;
        }

        if !exact_match && second_partial_match {
            self.print_error(options, d, &[b"ambiguous option -- ", name]);
            self.optopt = 0;
            return Some(step(BADCH));
        }
        let Some(match_index) = match_index else {
            if short_too {
                self.optind -= 1;
                return None;
            }
            self.print_error(options, d, &[b"unknown option -- ", name]);
            self.optopt = 0;
            return Some(step(BADCH));
        };
        let option = *longopts.get(match_index)?;
        if option.has_arg == NO_ARGUMENT && equal.is_some() {
            self.print_error(options, d, &[b"option doesn't take an argument -- ", name]);
            self.optopt = if option.flag_is_null { option.val } else { 0 };
            return Some(step(badarg(options)));
        }
        let mut optarg = OptArg::None;
        if option.has_arg == REQUIRED_ARGUMENT || option.has_arg == OPTIONAL_ARGUMENT {
            if equal.is_some() {
                optarg = OptArg::At {
                    arg_index: place_index,
                    offset: value_offset,
                };
            } else if option.has_arg == REQUIRED_ARGUMENT {
                let index = self.optind;
                self.optind += 1;
                if index >= 0 && index < len_i32(argv) {
                    optarg = OptArg::At {
                        arg_index: index as usize,
                        offset: 0,
                    };
                }
            }
        }
        if option.has_arg == REQUIRED_ARGUMENT && optarg == OptArg::None {
            self.print_error(options, d, &[b"option requires an argument -- ", name]);
            self.optopt = if option.flag_is_null { option.val } else { 0 };
            self.optind -= 1;
            return Some(step(badarg(options)));
        }
        Some(Step {
            code: if option.flag_is_null { option.val } else { 0 },
            optarg,
            long_index: Some(match_index),
            flag_to_set: if option.flag_is_null {
                None
            } else {
                Some(match_index)
            },
        })
    }

    fn print_error<D: Diag>(&self, options: &[u8], d: &mut D, parts: &[&[u8]]) {
        let print = self.opterr != 0 && options.first() != Some(&b':');
        if print {
            d.warnx(parts);
        }
    }

    fn place_byte<S: ArgSource>(&self, argv: &[S::Arg], src: &S) -> u8 {
        match self.place {
            Place::At { index, offset } => argv
                .get(index)
                .and_then(|arg| src.text(*arg).get(offset).copied())
                .unwrap_or(0),
            Place::Empty => 0,
        }
    }

    fn place_at_end<S: ArgSource>(&self, argv: &[S::Arg], src: &S) -> bool {
        match self.place {
            Place::At { index, offset } => argv
                .get(index)
                .is_none_or(|arg| offset >= src.text(*arg).len()),
            Place::Empty => true,
        }
    }

    fn advance_place<S: ArgSource>(&mut self, argv: &[S::Arg], src: &S) {
        if let Place::At { index, offset } = self.place {
            let length = argv.get(index).map_or(0, |arg| src.text(*arg).len());
            if offset < length {
                self.place = Place::At {
                    index,
                    offset: offset + 1,
                };
            } else {
                self.place = Place::At { index, offset };
            }
        }
    }

    fn permute_args<A: Copy>(
        &self,
        nonopt_start: i32,
        nonopt_end: i32,
        opt_end: i32,
        argv: &mut [A],
    ) {
        let nonopt_start = nonopt_start.max(0) as usize;
        let nonopt_end = nonopt_end.max(0) as usize;
        let opt_end = opt_end.max(0) as usize;
        if nonopt_end <= nonopt_start || opt_end <= nonopt_end {
            return;
        }
        let nnonopts = nonopt_end - nonopt_start;
        let nopts = opt_end - nonopt_end;
        let ncycle = gcd(nnonopts, nopts);
        if ncycle == 0 {
            return;
        }
        let cyclelen = (opt_end - nonopt_start) / ncycle;
        let mut i = 0usize;
        while i < ncycle {
            let cstart = nonopt_end + i;
            let mut pos = cstart;
            let mut j = 0usize;
            while j < cyclelen {
                if pos >= nonopt_end {
                    pos -= nnonopts;
                } else {
                    pos += nopts;
                }
                if let (Some(a), Some(b)) = (
                    argv.get_mut(pos).map(|value| *value),
                    argv.get(cstart).copied(),
                ) {
                    if let Some(slot) = argv.get_mut(pos) {
                        *slot = b;
                    }
                    if let Some(slot) = argv.get_mut(cstart) {
                        *slot = a;
                    }
                }
                j += 1;
            }
            i += 1;
        }
    }
}

impl Default for GetoptState {
    fn default() -> Self {
        Self::new()
    }
}

fn badarg(options: &[u8]) -> i32 {
    if options.first() == Some(&b':') {
        i32::from(b':')
    } else {
        BADCH
    }
}

fn gcd(a: usize, b: usize) -> usize {
    let mut a = a;
    let mut b = b;
    let mut c = a % b;
    while c != 0 {
        a = b;
        b = c;
        c = a % b;
    }
    b
}

fn len_i32<S>(argv: &[S]) -> i32 {
    i32::try_from(argv.len()).unwrap_or(i32::MAX)
}

fn step(code: i32) -> Step {
    Step {
        code,
        optarg: OptArg::None,
        long_index: None,
        flag_to_set: None,
    }
}

fn step_arg(code: i32, arg_index: usize, offset: usize) -> Step {
    Step {
        code,
        optarg: OptArg::At { arg_index, offset },
        long_index: None,
        flag_to_set: None,
    }
}

#[cfg(test)]
#[path = "getopt_ut.rs"]
mod tests;
