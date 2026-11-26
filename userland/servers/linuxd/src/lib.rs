// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `linuxd` core: the Linux process table, the B10 dispatcher wiring and the image loader.
//!
//! The server itself (`main.rs`) is thin: it registers `linux:`, reads a static Linux image from
//! the initfs, asks [`loader`] to build the process, marks it with `PROC_SET_PERSONALITY` and then
//! serves the kernel's trap packets with [`trap`].
//!
//! Everything host-testable lives here: the table, the syscall handlers, the trap codec and the
//! mapping plan. Nothing in this crate touches a syscall; `main.rs` owns the only `unsafe` code
//! (the raw `SCHEME_REGISTER`, `PROC_SET_PERSONALITY` and the scratch-mapping read).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod clone;
pub mod files;
pub mod handlers;
pub mod image;
pub mod loader;
pub mod procfs;
pub mod table;
pub mod trap;

pub use clone::{CloneKind, child_frame, classify, trampoline_page};
pub use files::{FileOp, is_synthetic, resolve_path, scheme_path};
pub use handlers::dispatcher;
pub use image::{
    Config, INTERP_BASE, ImageError, ImageSpec, LINUX_ROOT, MAIN_PIE_BASE, interp_path, plan_image,
};
pub use loader::{LoadError, Loaded, Sys, load};
pub use procfs::Resource;
pub use table::{
    CloneChild, CloneRequest, DirEntry, FLAG_ADDR, FileKind, LinuxProcess, OpenFile, Region,
    TRAMPOLINE_BASE, USER_BUFFER_MAX,
};
pub use trap::{TrapAction, TrapError, copy_in, decode, dispatch, encode_reply, lengths};

/// Convert a native error into the Linux syscall return value (`-errno`).
///
/// `linuxd` is the Linux personality server: every value it replies with is observed by a Linux
/// process. A native error number is translated through the donor conversion table; an error
/// outside the native range fails closed to `-EIO` instead of leaking a native number.
#[must_use]
pub fn linux_error(error: raven_abi::errno::Errno) -> isize {
    let linux = raven_linux_dispatch::xlat::linux_from_native(error.0)
        .unwrap_or(raven_linux_abi::errno::EIO);
    -(linux as isize)
}
