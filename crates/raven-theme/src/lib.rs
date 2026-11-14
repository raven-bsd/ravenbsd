// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD semantic palette, ANSI renderer and terminal adapters.
//!
//! The core is `no_std` + `alloc` and has no global state; `cargo build --no-default-features`
//! keeps it that way. The default `config` feature adds the std-only TOML loader for the
//! `raven` client; base tools keep the compiled palette.

#![cfg_attr(not(feature = "config"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod adapters;
pub mod ansi;
#[cfg(feature = "config")]
pub mod config;
pub mod decision;
pub mod palette;

pub use ansi::{Depth, paint};
pub use decision::Decision;
pub use palette::{Palette, Rgb, Role};

#[cfg(feature = "config")]
pub use config::ThemeFile;

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
