// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-libc-platform`: the POSIX platform layer of ravenlibc.
//!
//! Raven has handles, schemes and `CALL` messages; POSIX has file descriptors, paths, `stat`,
//! `poll` and `mmap`. This crate owns the translation. It is the only place above
//! `raven-libc-sys` that knows both worlds, and the only place below the C-ABI shell that knows
//! what an fd is.
//!
//! The design is fixed by `architecture/raven-libc/donor-map/11-syscall-platform.md`:
//!
//! * the fd table is process-local inside libc; each fd points to a refcounted open-file
//!   description (OFD) and each OFD owns exactly one kernel handle ([`fd`]);
//! * POSIX paths resolve once to an absolute lexical path and are then prefixed with `file:`
//!   ([`path`]);
//! * every kernel or server interaction goes through the [`SysBackend`] seam, implemented by
//!   [`RealSys`] (typed `raven-libc-sys` wrappers) and [`FakeSys`] (deterministic host tests);
//! * a primitive that does not exist yet is a registered class-A stub carrying its gap id
//!   ([`stubs`]), never a silent success and never an abort.
//!
//! The crate is `no_std` plus `alloc` and forbids `unsafe`: raw pointers, iovec arrays and the
//! C-ABI conversions belong to the shell (`raven-libc`, C60/C64), which hands this crate slices.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
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
)]

extern crate alloc;

pub mod backend;
pub mod compat;
pub mod conf;
pub mod dir;
pub mod fd;
pub mod intr;
pub mod io;
pub mod ioctl;
pub mod lock;
pub mod mem;
pub mod path;
pub mod platform;
pub mod poll;
pub mod random;
pub mod registry;
pub mod sched;
pub mod scheme;
pub mod stat;
pub mod stubs;
pub mod sysinfo;
pub mod time;
pub mod tty;
pub mod xlat;

#[cfg(test)]
#[path = "registry_ut.rs"]
mod registry_tests;

#[cfg(test)]
#[path = "stubs_ut.rs"]
mod stubs_tests;

pub use backend::{FakeSys, PendingCall, RealSys, SysBackend};
pub use fd::OfdKind;
pub use lock::{Lock, NoLock};
pub use platform::Platform;
pub use raven_abi::errno::Errno;
pub use raven_abi::types::Handle;
