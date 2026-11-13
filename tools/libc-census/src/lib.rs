// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ravenlibc API census: donor cells, rings, standards, ports gaps and missing
//! ranking.
//!
//! This is a host-only, offline tool. It reads donor checkouts and owner-provided
//! snapshots, emits facts (names and counts) and never copies donor prose or
//! code. The illumos `mapfile-vers` is parsed with the grammar of ticket C33
//! section 4.4; its text is never printed.

#![forbid(unsafe_code)]

pub mod cells;
pub mod merge;
pub mod missing;
pub mod model;
pub mod patches;
pub mod ports;
pub mod seed;
pub mod standards;
pub mod symlists;
pub mod table15;
pub mod testutil;
pub mod universe;
pub mod uses;
