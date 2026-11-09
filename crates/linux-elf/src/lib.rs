// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Linux process image setup rules.
//!
//! The crate computes the load biases, program-header addresses, entry point, `brk` start, TLS
//! information, auxiliary vector and the initial x86-64 stack for a Linux executable. It is pure
//! logic: no I/O, no kernel interfaces and no `unsafe`.
//!
//! The caller parses both the main executable and an optional interpreter with [`raven_elf::Elf`],
//! then passes them to [`plan_image`]. [`build_auxv`] produces the auxiliary vector, and
//! [`build_stack`] writes the byte-exact initial stack below a caller-provided top address.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod auxv;
mod error;
mod plan;
mod stack;

pub use auxv::{Auxv, build_auxv};
pub use error::LinuxElfError;
pub use plan::{Config, ImagePlan, LoadPlanSummary, TlsInfo, plan_image};
pub use stack::{StackImage, build_stack};
