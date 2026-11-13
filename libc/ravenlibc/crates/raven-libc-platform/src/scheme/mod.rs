// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thin encoders over the `raven-proto-*` crates and the cached root handle.
//!
//! Nothing here parses a POSIX path as a scheme. `file:` is added only after the path layer
//! produced an absolute path; native schemes are reached through `raven_open_scheme` in the C
//! shell, which calls [`client::open_scheme_path`] with an already-qualified scheme path.

pub mod client;
pub mod probe;
pub mod rootcall;

pub use client::{decode_fs_response, encode_fs_call, fs_call, open_scheme_path};
pub use probe::probe_kind;
pub use rootcall::RootCall;

#[cfg(test)]
#[path = "scheme_ut.rs"]
mod tests;
