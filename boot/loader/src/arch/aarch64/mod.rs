// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 loader support.
//!
//! The page-table encoder is compiled on the host as well so its tests run in
//! `cargo test -p raven-loader`. The handoff module uses aarch64 system registers and is only
//! compiled for the target.

#[cfg(any(target_arch = "aarch64", test))]
pub mod paging;

#[cfg(target_arch = "aarch64")]
pub mod handoff;
