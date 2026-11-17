// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `uucp`, `uustat` and `uux` spool model.
//!
//! The kernel has no network transport in this image. The local queue under `/var/spool/uucp` is
//! the tested surface: `uucp` copies local files or queues remote copies, `uux` queues a remote
//! command, and `uustat` lists, summarizes or cancels the queue. The first-party record format is
//! documented in `docs/posix/P4-uucp.md`; it is not a wire protocol.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod config;
pub mod job;
pub mod queue;
pub mod spool;
pub mod uucp;

#[cfg(target_os = "none")]
pub mod runtime;

pub use config::{Config, ConfigError};
pub use job::{Job, JobError, JobKind, NamedFile};
pub use queue::{QueueError, Summary};
pub use spool::{Spool, SpoolError};
pub use uucp::{Address, Options, UsageError, UucpError, parse_args, run, split_address};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
