// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc `printf` and `scanf` engines.
//!
//! This crate is the pure-logic engine behind every formatted I/O call of the C library
//! (`printf`, `snprintf`, `dprintf`, `asprintf`, `syslog`, `err`, `%m`, wide `swprintf`,
//! `scanf`, `sscanf`, wide `swscanf`). It is a pair of functions over traits:
//!
//! * [`vformat`] writes to a [`Sink`], reads arguments from a [`VaReader`], dereferences
//!   pointers through [`FmtEnv`] and asks a [`FloatFmt`] implementation for floating-point
//!   digits. It never sees a `FILE`, never touches `errno`, never allocates and contains no
//!   `unsafe` code.
//! * [`vscan`] reads from a [`ScanSource`], stores through [`FmtEnv`] and asks a
//!   [`FloatScan`] implementation to convert a validated token.
//!
//! The float work is a hook: the crate contains no `dtoa` symbol and no floating-point
//! instruction, so an integer-only instantiation links no numeric code. [`NoFloat`] and
//! [`StubFloat`] are the two dummy hooks of `donor-map/04` 3.8 and `donor-map/16` R-13;
//! [`NoScanFloat`] is the matching-failure hook for `scanf`.
//!
//! The integer conversion logic and the positional argument table are transpiled from
//! FreeBSD `lib/libc/stdio/{vfprintf.c,printfcommon.h,printflocal.h,printf-pos.c}`; the
//! scanf engine is transpiled from `musl:src/stdio/vfscanf.c` and its helpers; the float
//! syntax lexer is transpiled from `bionic:libc/stdio/parsefloat.c`. Per-file provenance is
//! in the header of each module. The area document is
//! `architecture/raven-libc/donor-map/04-stdio.md` (sections 3.2, 3.3 and 3.8) and
//! `05-numerics.md` 3.2.

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

#[cfg(test)]
extern crate self as raven_libc_fmt;

pub mod glue;
pub mod printf;
pub mod scanf;
pub mod sinks;
pub mod traits;
pub mod types;

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "../tests/support/mod.rs"]
pub mod test_support;

pub use printf::{vformat, vformat_dry};
pub use scanf::{Limited, ScanEnvFloat, ScanSource, vscan};
pub use sinks::{BoundedSink, CountingSink, SliceSink};
pub use traits::{
    CharUnit, FloatFmt, FloatScan, FmtEnv, NoFloat, NoScanFloat, Sink, SinkError, StubFloat,
    VaReader,
};
pub use types::{
    C_NUMERIC_FORMAT, DEC_BUF_F64, DEC_BUF_LD, DigitsOut, FloatBits, FloatClass, FloatKind, FmtErr,
    FmtOptions, NL_ARGMAX, NumericFormat, PctN, SCAN_TOKEN_MAX, ScanOptions, ScanResult, StoreSize,
};
