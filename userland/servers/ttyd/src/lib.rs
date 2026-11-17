// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable session/foreground-terminal state.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod peers;
pub mod sessions;

pub use peers::{Peer, Peers};
pub use sessions::{Session, Sessions};
