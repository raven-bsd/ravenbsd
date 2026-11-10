// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Architecture-specific page-table constants.
//!
//! The x86-64 backend is the original S06 port. The aarch64 backend (S14a) implements the same
//! encoding trait with the ARMv8-A 4 KiB-granule descriptor format.

pub mod aarch64;
pub mod x86_64;
