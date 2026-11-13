// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `<stdlib.h>`, `<search.h>`, `<getopt.h>`, `<err.h>`, `<assert.h>` and `<libgen.h>` logic.
//!
//! This is the pure-logic crate of `architecture/raven-libc/donor-map/03-stdlib.md`: integer
//! conversion, sorting and searching, the three random generators, the exit machinery, the
//! environment, option parsing, trees and hash tables, `realpath`, and the text of `assert` and
//! `err`. Every C pointer, `errno` store, lock, `atexit` function pointer and `environ` array is
//! the shell's (`raven-libc`, C60, R-17-4); the engines here take slices and return `Result`.
//!
//! The crate is `no_std` plus `alloc` (R-8) and class P: `#![forbid(unsafe_code)]`. It never
//! formats with `core::fmt` (R-5), never reads the environment or the clock, and never allocates
//! in `qsort`, `bsearch`, `strtol`, `getenv` or the first 32 `atexit` registrations (R-17).

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

pub mod arith;
pub mod conv;
pub mod diag;
pub mod env;
pub mod exit;
pub mod getopt;
pub mod mb;
pub mod misc;
pub mod path;
pub mod proc;
pub mod rand;
pub mod search;
pub mod sort;

pub use conv::{IntStatus, Scan, atoi, atol, scan_integer, strtol, strtonum, strtoul};
pub use raven_libc_abi::errno::Errno;
pub use sort::{SortError, bsearch, heapsort, mergesort, qsort};

/// One code unit of a numeric string: `strtol` uses `u8`, `wcstol` uses `u32` (R-10: exactly
/// these two instantiations).
pub trait CodeUnit: Copy + Eq {
    /// The numeric value of the code unit.
    fn to_u32(self) -> u32;
}

impl CodeUnit for u8 {
    fn to_u32(self) -> u32 {
        u32::from(self)
    }
}

impl CodeUnit for u32 {
    fn to_u32(self) -> u32 {
        self
    }
}
