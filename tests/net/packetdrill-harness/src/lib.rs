// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host harness that drives `raven-netcore` with a small packetdrill-compatible script subset.
//!
//! The crate is not a packetdrill replacement: it parses a curated subset of packetdrill's
//! script syntax and feeds the packets directly into a `raven-netcore` stack over the
//! in-memory `PairDevice` link. packetdrill itself (GPL-2.0) stays an external tool and no
//! packetdrill code or test script is used here.

#![forbid(unsafe_code)]
// The harness is an internal test binary; its types mirror the script syntax.
#![allow(missing_docs)]

pub mod engine;
pub mod frame;
pub mod script;

pub use engine::Runner;
pub use script::{Script, parse};
