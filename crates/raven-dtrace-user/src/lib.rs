// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DTrace userland consumer and USDT helpers.
//!
//! This crate is the userspace half of the S57 kernel provider: it owns the `DTRACE_*` syscall
//! numbers and wire records, a consumer wrapper that drives a [`consumer::TraceKernel`] backend,
//! record formatting for the `dtrace` utility and the USDT nop-site marker/note parser. The
//! kernel provider is not edited here; the marker path that would fire a user probe needs a
//! follow-up kernel enabler.

#![cfg_attr(target_os = "none", no_std)]

pub mod abi;
pub mod args;
pub mod consumer;
pub mod usdt;

#[cfg(target_os = "none")]
pub mod sys;

pub use abi::{CpuStats, PROBE_LIST, ProbeInfo, Record};
pub use args::{ArgError, Options, parse_args};
pub use consumer::{Consumer, TraceKernel, format_record};
pub use usdt::{
    NoteError, USDT_PROBES, UsdtNote, UsdtProbe, parse_note, site_address, site_opcode,
};

pub use raven_abi::errno::Errno;
