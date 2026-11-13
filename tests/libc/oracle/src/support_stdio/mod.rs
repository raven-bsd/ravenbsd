// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdio-musl` differential support: the shared in-memory file system, the C hook table, the
//! operation-script case and the two runners.
//!
//! The C original runs through the replacement headers of `shim/stdio/musl/`: every
//! `__syscall` goes to [`hooks`], which forwards to the same [`store`] the Rust subject's
//! [`raven_libc_stdio::SysFile`] implementation uses. Reads and buffer-allocation probes are
//! excluded from the compared trace: [`raven_libc_stdio::SysFile`] has no `readv`, so the
//! subject issues two reads where the donor issues one (declared in the work list), and the
//! donor probes the tty with `ioctl` while the subject calls `fstat`+`isatty`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

pub mod case;
pub mod generate;
pub mod hooks;
pub mod memcase;
pub mod store;

pub use case::{Obs, ScriptCase, ScriptOp, run_oracle, run_subject, state};
pub use hooks::Hooks;

/// One test at a time may drive the shared store and the global subject state.
pub fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .expect("test lock")
}
