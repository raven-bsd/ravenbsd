// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD output contract shared by `raven` and the base utilities.
//!
//! The crate is `no_std` + `alloc` and has no global state. A [`Record`] is one NDJSON object
//! with the mandatory `schema` and `kind` fields; [`JsonWriter`] frames one or more records as
//! one object per line. [`ErrorEnvelope`] is the stderr form, [`Exit`] is the 0/1/2 status
//! contract and [`Color`] is the human-only colour policy. JSON never carries colour codes.
//!
//! The wire contract is `architecture/17-modern-userland.md` section 2.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod color;
mod error;
mod exit;
mod format;
mod json;
mod record;

pub use color::Color;
pub use error::ErrorEnvelope;
pub use exit::Exit;
pub use format::Format;
pub use json::{JsonWriter, escape_json, escaped};
pub use record::{Record, Value};

/// The NDJSON schema version emitted by every record.
pub const SCHEMA: u64 = 1;
