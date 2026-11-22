// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD PID 1 (`svcd`) and its host-testable supervision core.
//!
//! The [`state`] module is a pure state machine: it has no syscalls, clock or threads and is
//! exercised by the host tests. The target-only modules wire it to `raven-rt`: mapping the initfs,
//! parsing `/etc/services.d/*.toml`, spawning children, serving the `svc:` scheme and supervising
//! restarts. The crate keeps `#![forbid(unsafe_code)]`; all unsafe remains in `raven-rt`.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod bootscreen;
pub mod config;
pub mod grants;
pub mod health;
pub mod state;

#[cfg(target_os = "none")]
mod boot;
#[cfg(target_os = "none")]
mod legacy;
#[cfg(target_os = "none")]
mod svc;
#[cfg(target_os = "none")]
mod worker;
#[cfg(target_os = "none")]
pub use worker::{WAIT_ERROR_CODE, wait_child_status};

/// Select the plain boot transcript when the caller has `NO_COLOR` or `TERM=dumb`.
pub fn configure_boot_plain(no_color: bool, term_dumb: bool) {
    bootscreen::set_plain(no_color || term_dumb);
}

/// Boot the service manager on a real target and return the process exit code.
#[cfg(target_os = "none")]
pub fn run() -> i32 {
    boot::run()
}
