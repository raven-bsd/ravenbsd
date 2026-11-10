// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable scheme-IPC policy.
//!
//! The packet codec and the namespace/rights model live in the kernel tree so the kernel and
//! this crate compile exactly the same source. This crate supplies the `alloc` environment and
//! the host tests; it has no kernel dependencies beyond `raven-abi`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[path = "../../../kernel/src/ipc/model.rs"]
pub mod model;
#[path = "../../../kernel/src/ipc/protocol.rs"]
pub mod protocol;
