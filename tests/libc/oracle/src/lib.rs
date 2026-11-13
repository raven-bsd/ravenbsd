// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential oracle harness for ravenlibc (ticket C35).
//!
//! The harness compares a Rust subject with the donor C original compiled by `libc-oracle` on the
//! same generated inputs: a fixed corpus, a seeded pseudo-random stream and small exhaustive
//! domains. Observations are compared whole: return values and every byte of every output buffer,
//! including the guard bytes the C must not touch.
//!
//! The C oracle is linked as a shared object through `#[link]` declarations in [`decls`]; the
//! integration tests need `LIBRARY_PATH` and `LD_LIBRARY_PATH` pointing at the directory printed by
//! `libc-oracle build` (the `cargo xtask libc-oracle diff <family>` command does that).
//!
//! `#![deny(unsafe_code)]` covers the crate; only [`decls`] (extern declarations) and [`call`] (the
//! calls and the errno access) are allowed to contain unsafe code.

#![deny(unsafe_code)]

#[allow(unsafe_code)]
pub mod call;
pub mod config;
pub mod corpus;
#[allow(unsafe_code)]
pub mod decls;
pub mod diff;
pub mod mutant;
pub mod norm;
pub mod rng;
pub mod shrink;
pub mod standin;
#[allow(unsafe_code)] // calls the C variadic oracle through function pointers
pub mod support_fmt;
#[cfg(feature = "oracle-stdio-musl")]
#[allow(unsafe_code)] // installs the C hook table and calls the oracle FILE functions
pub mod support_stdio;
