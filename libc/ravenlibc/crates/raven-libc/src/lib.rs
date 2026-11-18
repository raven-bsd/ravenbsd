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

// C65's wrappers were x86_64-only while their `c_char` handling assumed `i8`. The generic
// compat/IPC exports now build on aarch64 and riscv64 too; the only assembly in the tree
// (backtrace) has its own per-arch `#[cfg]`, so the wrappers themselves stay portable.
// `c_char` is `u8` on aarch64/riscv64, which the path buffers below spell as `c_char` rather
// than `i8`.
#[cfg(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
))]
pub mod compat;
#[cfg(target_os = "none")]
pub(crate) mod ctype;
#[cfg(target_os = "none")]
pub(crate) mod dl;
#[cfg(target_os = "none")]
pub(crate) mod errno;
#[cfg(target_os = "none")]
pub(crate) mod setjmp;
#[cfg(target_os = "none")]
pub(crate) mod signal;
#[cfg(target_os = "none")]
pub(crate) mod threads;
#[cfg(target_os = "none")]
pub(crate) mod ucontext;
// The exit registry lives in `stdlib/` but is declared here so its host unit tests build even
// though `stdlib` is target-only (C61 integration).
#[cfg(any(target_os = "none", test))]
#[path = "stdlib/atexit.rs"]
pub(crate) mod exit_registry;
// C64d's pure iovec and fcntl-argument helpers are host-tested without the target platform.
#[cfg(test)]
#[path = "unistd/fcntl_args.rs"]
mod fcntl_args_host;
#[cfg(test)]
#[path = "unistd/readv.rs"]
mod readv_host;
#[cfg(test)]
#[path = "unistd/writev.rs"]
mod writev_host;
// C64d's `realpath` path walk and the no-unwinder ABI surface are host-tested even though
// their C exports are target-only.
pub(crate) mod complex;
pub(crate) mod fcntl;
pub(crate) mod fenv;
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
pub(crate) mod locale;
#[cfg(target_os = "none")]
pub(crate) mod malloc;
#[cfg(target_os = "none")]
pub(crate) mod math;
#[cfg(target_os = "none")]
pub(crate) mod netdb;
#[cfg(target_os = "none")]
pub(crate) mod panic;
// P1c's ABI-ready exports. The host build compiles the module for its unit tests too; the
// `no_mangle` attribute is target-only.
#[cfg(any(target_os = "none", test))]
pub(crate) mod p1c_exports;
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
pub(crate) mod pthread;
#[cfg(target_os = "none")]
pub(crate) mod pwd;
#[cfg(target_os = "none")]
#[path = "arc4random_buf.rs"]
pub(crate) mod rand;
#[cfg(any(target_os = "none", test))]
#[path = "stdlib/realpath.rs"]
pub(crate) mod realpath;
#[cfg(target_os = "none")]
pub(crate) mod regex;
#[cfg(target_os = "none")]
pub(crate) mod sched;
#[cfg(target_os = "none")]
pub(crate) mod socket;
#[cfg(any(target_os = "none", test))]
#[path = "stdlib/unwind/mod.rs"]
pub(crate) mod unwind;
// The guard storage is also compiled for the host unit test; it stops exporting the C name on
// the host (the archive check covers the real symbol).
#[cfg(any(target_os = "none", test))]
#[path = "__stack_chk_guard.rs"]
pub(crate) mod stack_guard;
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
pub(crate) mod syslog;
#[cfg(target_os = "none")]
pub(crate) mod time;
#[cfg(target_os = "none")]
pub(crate) mod unistd;
pub mod va;
#[cfg(target_os = "none")]
pub(crate) mod wchar;
#[cfg(target_os = "none")]
pub(crate) mod wctype;
