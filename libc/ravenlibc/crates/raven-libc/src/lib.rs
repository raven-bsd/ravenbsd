// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc C-ABI shell.
//!
//! This crate owns every `extern "C"` export of ravenlibc, the `global_asm!` variadic
//! trampolines and the `va_list` readers. The bodies are forwarders: one logic-crate call, the
//! argument conversions of [`ffi`] and the `errno` write of [`ffi::ret`] (C60 4.3). The logic
//! lives in the `raven-libc-*` crates; nothing here implements an algorithm except the hot
//! functions of `hot.toml`, which are call-free loops in their own member by design
//! (`donor-map/16` R-1/R-15).
//!
//! Class `U` of `17-crate-layout.md` section 1 (L-3): the shell may use `unsafe`; every block
//! carries a `// SAFETY:` comment (workspace lint `undocumented_unsafe_blocks`). `ffi` is
//! compiled on the host too so its conversions are unit-tested; every export module is
//! `#[cfg(target_os = "none")]` because on the host its symbols collide with the host libc.
#![cfg_attr(not(test), no_std)]
#![no_builtins]
#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are documented build-profile cfgs
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

extern crate alloc;

// The host build of the staticlib needs std's panic handler and global allocator; the target
// build is the real `no_std` artifact (C60 4.1) and provides its own under `panic-handler`.
#[cfg(not(target_os = "none"))]
extern crate std;

// C65's wrappers assume `c_char == i8`; they build on x86_64 only (the other arches list
// them as DEFERRED until the C65 follow-up).
#[cfg(target_arch = "x86_64")]
pub mod compat;
#[cfg(target_os = "none")]
pub(crate) mod ctype;
#[cfg(target_os = "none")]
pub(crate) mod errno;
pub(crate) mod ffi;
#[cfg(target_os = "none")]
pub(crate) mod fts;
#[cfg(target_os = "none")]
pub(crate) mod glob;
#[cfg(target_os = "none")]
pub(crate) mod grp;
#[cfg(target_os = "none")]
pub(crate) mod hooks;
#[cfg(target_os = "none")]
pub(crate) mod malloc;
#[cfg(target_os = "none")]
pub(crate) mod math;
#[cfg(target_os = "none")]
pub(crate) mod netdb;
#[cfg(target_os = "none")]
pub(crate) mod panic;
#[cfg(target_os = "none")]
pub(crate) mod personality_bsd;
#[cfg(target_os = "none")]
pub(crate) mod personality_common;
#[cfg(target_os = "none")]
pub(crate) mod personality_linux;
#[cfg(all(target_os = "none", feature = "personality-solaris"))]
pub(crate) mod personality_solaris;
#[cfg(target_os = "none")]
pub(crate) mod platform;
#[cfg(target_os = "none")]
pub(crate) mod process;
#[cfg(target_os = "none")]
pub(crate) mod pwd;
#[cfg(target_os = "none")]
pub(crate) mod regex;
#[cfg(target_os = "none")]
pub(crate) mod sched;
#[cfg(target_os = "none")]
pub(crate) mod start;
#[cfg(target_os = "none")]
pub(crate) mod stdio;
#[cfg(target_os = "none")]
pub(crate) mod stdlib;
#[cfg(target_os = "none")]
pub(crate) mod string;
#[cfg(target_os = "none")]
pub(crate) mod strings;
#[cfg(target_os = "none")]
pub(crate) mod symver;
#[cfg(target_os = "none")]
pub(crate) mod time;
#[cfg(target_os = "none")]
pub(crate) mod unistd;
pub mod va;
#[cfg(target_os = "none")]
pub(crate) mod wchar;
