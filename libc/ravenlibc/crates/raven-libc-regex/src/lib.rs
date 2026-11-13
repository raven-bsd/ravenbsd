// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Spencer regular expressions for ravenlibc (`architecture/raven-libc/donor-map/07`).
//!
//! This crate is the pure-logic port of the NetBSD Spencer engine: `regcomp`, `regexec`,
//! `regerror`, `regfree`-as-`Drop`, `regnsub` and `regasub`, the BSD extension flags
//! (`REG_STARTEND`, `REG_NOSPEC`, `REG_PEND`, `REG_GNU`, `REG_BACKR`, `REG_LARGE`) and both
//! matcher families. It is `no_std` + `alloc`, class P (`forbid(unsafe_code)`), has no global
//! state and no clock or randomness. The C-ABI shell owns `regex_t`, `regmatch_t` and the
//! exported symbols; wide-character access goes through [`locale::RegexLocale`] until the
//! locale crate (C48) exists.
//!
//! Hardening on top of the donor: [`consts::MAX_STRIP`] (H-RE-1), [`consts::BACKREF_BUDGET`]
//! with [`consts::BACKREF_DEPTH_MAX`] (H-RE-2) and [`consts::MAX_PAREN_DEPTH`] (H-RE-3).
//! Every limit reports `REG_ESPACE`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

pub mod cname;
pub mod consts;
pub mod engine;
pub mod locale;
pub mod regcomp;
pub mod regerror;
pub mod regex2;
pub mod regexec;
pub mod regfree;
pub mod regsub;
pub mod utils;

use crate::consts::MAGIC1;
use crate::regex2::ReGuts;

pub use locale::{ByteLocale, MbResult, RegexLocale, Utf8Locale, Wctype};
pub use regcomp::regcomp;
pub use regerror::{regatoi, regatoi_str, regerror};
pub use regexec::regexec;
pub use regsub::{regasub, regnsub};

/// One sub-match: `-1` in either field means "did not participate" (`regoff_t` is `i64`).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct RegMatch {
    /// Start offset of the match.
    pub rm_so: i64,
    /// End offset of the match.
    pub rm_eo: i64,
}

/// One of the `REG_*` error codes; never 0.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct RegError(pub i32);

/// A compiled regular expression (the donor's `re_guts` plus the outer magic word).
///
/// The value is `Send + Sync`: it has no interior mutability, and the per-call `struct match`
/// of `regexec` is local to that call.
#[derive(Clone)]
pub struct Regex {
    /// `MAGIC1` while the value is valid.
    pub(crate) magic1: i32,
    /// The compiled program.
    pub(crate) guts: ReGuts,
}

impl Regex {
    /// `re_nsub`: the number of parenthesised subexpressions.
    #[must_use]
    pub fn nsub(&self) -> usize {
        self.guts.nsub
    }

    /// The number of strip words (`nstates`); used by the dispatch and limit tests.
    #[must_use]
    pub fn nstates(&self) -> usize {
        self.guts.nstates as usize
    }

    /// The `regfree` guard: both magic words are intact.
    #[must_use]
    pub fn magic_ok(&self) -> bool {
        self.magic1 == MAGIC1 && self.guts.magic_ok()
    }

    /// A zeroed object, for the "zeroed `regex_t` must not panic" test.
    ///
    /// The shell can receive all-zero bytes; this is the same shape without `unsafe`.
    #[cfg(test)]
    pub(crate) fn zeroed_for_test() -> Self {
        Self {
            magic1: 0,
            guts: ReGuts::default(),
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
