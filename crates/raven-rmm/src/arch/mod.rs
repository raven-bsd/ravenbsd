// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Architecture-specific page-table constants.
//!
//! Both backends expose the same constant surface; descriptor decisions live in
//! [`crate::page::Encoding`].

pub mod aarch64;
pub mod x86_64;
