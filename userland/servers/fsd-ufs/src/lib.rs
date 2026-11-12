// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Read-only UFS filesystem server.
//!
//! [`view::UfsView`] adapts `ufs-core` to the small [`handler::FileSystem`]
//! interface, and [`handler::FsHandler`] serves that interface over the `file:`
//! scheme.  Both layers are host-testable; `main.rs` adds the `block:` client
//! and the S11 server loop for the target.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

pub mod block;
pub mod handler;
pub mod view;

pub use handler::{FileSystem, FsHandler, MAX_OPEN_FILES};
pub use view::UfsView;
