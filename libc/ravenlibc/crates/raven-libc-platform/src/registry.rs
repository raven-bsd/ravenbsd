// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The T1-T9 function registry behind `FUNCTIONS.md`.
//!
//! The table is the machine-readable copy of the document; `registry_ut` parses
//! `FUNCTIONS.md` and fails when the two disagree, when an `EXISTS`/`PARTIAL` row has no
//! implementation in the source tree, or when a `TBD` row is missing from [`crate::stubs`].

/// Implementation status verified against this repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionStatus {
    /// Works with the kernel and servers at this revision.
    Exists,
    /// Works with the deviation named in the row.
    Partial,
    /// Registered class-A stub.
    Tbd,
    /// Not exported by `libc.so.1` (class B) or owned by another ticket.
    Skip,
}

impl FunctionStatus {
    /// The document spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exists => "EXISTS",
            Self::Partial => "PARTIAL",
            Self::Tbd => "TBD",
            Self::Skip => "SKIP",
        }
    }
}

/// One row of the function table.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// POSIX name.
    pub name: &'static str,
    /// Raven primitive today (`S:NAME`, `CALL(OP_X)`, `L`, `FD`, `none`).
    pub primitive: &'static str,
    /// Verified status.
    pub status: FunctionStatus,
    /// Gap id (`PG-n`) or a ticket name.
    pub gap: &'static str,
    /// Owner lane or ticket.
    pub owner: &'static str,
    /// Donor reference.
    pub donor: &'static str,
    /// Implemented outside this crate (another ticket owns it).
    pub external: bool,
}

include!("registry_table.rs");
