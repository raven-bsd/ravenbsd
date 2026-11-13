// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `snprintf`/`swprintf` glue.
//!
//! The glue owns the buffer-limit rules; the conversion logic is [`crate::vformat`]. This is
//! the CLEANROOM part of `donor-map/04` 5.2: `cap == 0` writes nothing and returns the full
//! length; `cap > 0` always NUL-terminates and returns the length the full output would
//! have had; `swprintf` fails with [`crate::FmtErr::Overflow`] on truncation and with
//! [`crate::FmtErr::Invalid`] when `cap == 0`.

pub mod snprintf;
pub mod swprintf;

pub use snprintf::vsnprintf_core;
pub use swprintf::vswprintf_core;
