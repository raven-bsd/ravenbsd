// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! packetdrill-style runner for the native `raven-tcpip` stack.

#![forbid(unsafe_code)]

pub mod packet;
pub mod runner;
pub mod script;
pub mod table;

pub use runner::{RunResult, Runner, run_text};
pub use script::{Step, Syscall, parse};
