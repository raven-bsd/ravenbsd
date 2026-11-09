// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Strict, deterministic initfs boot-archive reader and builder.
//!
//! The initfs is the single blob the boot path hands to the kernel. It holds `/init` and the first
//! user programs, so both sides must agree on one small, fully validated format (CONTRACTS C7).
//!
//! The reader ([`Initfs`]) borrows the image, never allocates and never trusts it: [`Initfs::parse`]
//! validates the header, the entry table, the names blob, every path, every data range and every
//! CRC up front. After that all accessors are total. The builder ([`InitfsBuilder`], feature `std`)
//! writes the canonical layout: header, entry table, names blob, page padding, then data regions in
//! entry order, each padded to a page. Entries are sorted by name bytes, so the output depends only
//! on the set of `(path, data, mode)` tuples and two builds of the same input are byte-identical.
//!
//! The crate contains no `unsafe` code and no dependencies.

#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![forbid(unsafe_code)]

mod crc;
mod format;
mod reader;

#[cfg(feature = "std")]
mod builder;

#[cfg(test)]
mod testutil;

pub use crc::crc32;
pub use reader::{Entry, Initfs, InitfsError};

#[cfg(feature = "std")]
pub use builder::{BuildError, InitfsBuilder};
