// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD UEFI loader library.
//!
//! The `config` module is pure and host-testable; `fs`, `image` and `serial` need UEFI boot
//! services and are compiled only for the `uefi` target.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod config;

#[cfg(target_os = "uefi")]
pub mod fs;
#[cfg(target_os = "uefi")]
pub mod image;
#[cfg(target_os = "uefi")]
pub mod serial;
