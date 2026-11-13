// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Registered class-A stubs.
//!
//! A function whose primitive does not exist is exported as `-1`/`errno` with its gap id, never
//! as a fake success and never as `abort()`. The C shell reads this registry (and its generated
//! `.raven_unimpl` records from `raven-libc-sys`) to build `<raven/stubs.h>`.

use raven_abi::errno::Errno;

/// One registered stub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stub {
    /// POSIX name.
    pub name: &'static str,
    /// Gap id (`PG-n`).
    pub gap: &'static str,
    /// Errno the stub reports.
    pub errno: u16,
}

impl Stub {
    /// The errno as a typed value.
    pub const fn error(&self) -> Errno {
        Errno(self.errno)
    }
}

/// Look a stub up by name.
pub fn lookup(name: &str) -> Option<&'static Stub> {
    STUBS.iter().find(|stub| stub.name == name)
}

/// The errno a registered stub reports, or `None` when the name is not a stub.
pub fn stub_errno(name: &str) -> Option<Errno> {
    lookup(name).map(Stub::error)
}

include!("stubs_table.rs");
