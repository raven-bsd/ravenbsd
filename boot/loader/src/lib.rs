// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD UEFI loader library.
//!
//! The `config` module is pure and host-testable; `fs`, `image` and `serial` need UEFI boot
//! services and are compiled only for the `uefi` target.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod arch;
pub mod bootinfo;
pub mod config;
pub mod console_mirror;
#[cfg(target_os = "uefi")]
pub mod fs;
#[cfg(target_os = "uefi")]
pub mod handoff;
pub mod health;
#[cfg(target_os = "uefi")]
pub mod image;
pub mod layout;
pub mod memmap;
pub mod paging;
pub mod paging_core;
#[cfg(target_os = "uefi")]
pub mod serial;
