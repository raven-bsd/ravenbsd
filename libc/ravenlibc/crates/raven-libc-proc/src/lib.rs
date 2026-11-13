// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure process logic for `ravenlibc`.
//!
//! On Raven the kernel has no `fork`, `exec`, pids or signals: the process API is a thin client
//! of the `procd` userspace server. This crate holds the parts that are pure logic and can run on
//! the host: the `exec` PATH search, the `posix_spawn` attribute and file-action model, the Linux
//! wait-status codec, the `wait4`/`waitid` mapping, the `popen` pid list, the `atfork` and fork
//! lock registries, the rlimit table, the startup stack parser, the procinfo page reader and the
//! client-side encoders of the proposed `procd` protocol.
//!
//! The crate is `no_std` plus `alloc`, class P and contains no `unsafe` token.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)] // tests use fixed vectors and fail by panicking

extern crate alloc;

pub mod atfork;
pub mod auxv;
pub mod backend;
pub mod consts;
pub mod daemon;
pub mod errors;
pub mod exec_path;
pub mod fake;
pub mod getauxval;
pub mod getprogname;
pub mod ids;
pub mod initgroups;
pub mod limits;
pub mod local_backend;
pub mod lock_registry;
pub mod nice;
pub mod popen_model;
pub mod procinfo;
pub mod rlimit_model;
pub mod setprogname;
pub mod spawn_model;
pub mod spawnp;
pub mod startup;
pub mod times;
pub mod wait;
pub mod wait3;
pub mod wait_status;
pub mod waitid;
pub mod waitpid;
pub mod wire;

pub use errors::Errno;
