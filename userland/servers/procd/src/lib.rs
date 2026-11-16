// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable process-model state for `procd`.
//!
//! The kernel remains the authority for live process identity, address spaces and readiness. This
//! table records only the children `procd` created, their parent/owner connection and the wait
//! status it has already observed. That keeps the server honest: it cannot invent a process it
//! does not own, and it cannot query or signal an arbitrary caller.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod model;

pub use model::{
    Child, P_ALL, P_PID, ProcInfo, ProcSys, Procd, SIGINT, SIGKILL, SIGTERM, WNOHANG, WaitStatus,
    encode_wait_status,
};
