// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD DNS resolver service.
//!
//! The resolver core is host-testable: DNS wire parsing, `/etc/resolv.conf`
//! parsing and the A/AAAA query loop live in library modules.  `main.rs` adds
//! the `resolv:` scheme and the `net:` UDP transport for the target.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod config;
pub mod dns;
pub mod proto;
pub mod resolver;
