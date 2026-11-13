// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The BSD `uuid` API (donor-map 15 table T3, 02 X51, doc 15 4.4).
//!
//! `uuid_t` is the 16-byte `struct uuid` of `freebsd:include/uuid.h` (`time_low`, `time_mid`,
//! `time_hi_and_version`, `clock_seq_hi_and_reserved`, `clock_seq_low`, `node[6]`). The
//! functions come from the NetBSD `lib/libc/uuid` files (BSD-2; the FreeBSD checkout has no
//! `lib/libc/uuid`). `uuid_create` is CLEANROOM: the donor calls `uuidgen(2)`, which Raven does
//! not have, so it builds a version-4 UUID from [`crate::seams::Entropy`] (RD-new).
//!
//! `uuid_generate`/`uuid_unparse` (libuuid) are **not** here: they are an add-on port and there
//! is no `-luuid` stub (02 R12, doc 15 4.4).

pub mod compare;
pub mod create;
pub mod equal;
pub mod hash;
pub mod nil;
pub mod stream;
pub mod string;

pub use compare::compare;
pub use create::{create, create_nil};
pub use equal::equal;
pub use hash::hash;
pub use nil::is_nil;
pub use stream::{dec_be, dec_le, enc_be, enc_le};
pub use string::{from_string, to_string};

/// The `uuid_s_*` status codes of `uuid.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Status {
    /// `uuid_s_ok`.
    Ok = 0,
    /// `uuid_s_bad_version`.
    BadVersion = 1,
    /// `uuid_s_invalid_string_uuid`.
    InvalidString = 2,
    /// `uuid_s_no_memory`.
    NoMemory = 3,
}

/// A DCE 1.1 UUID (`struct uuid`, 16 bytes, align 4).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Uuid {
    /// `time_low`.
    pub time_low: u32,
    /// `time_mid`.
    pub time_mid: u16,
    /// `time_hi_and_version`.
    pub time_hi_and_version: u16,
    /// `clock_seq_hi_and_reserved`.
    pub clock_seq_hi_and_reserved: u8,
    /// `clock_seq_low`.
    pub clock_seq_low: u8,
    /// `node[6]`.
    pub node: [u8; 6],
}

const _: () = assert!(core::mem::size_of::<Uuid>() == 16);
const _: () = assert!(core::mem::align_of::<Uuid>() == 4);
const _: () = assert!(core::mem::offset_of!(Uuid, time_low) == 0);
const _: () = assert!(core::mem::offset_of!(Uuid, time_mid) == 4);
const _: () = assert!(core::mem::offset_of!(Uuid, time_hi_and_version) == 6);
const _: () = assert!(core::mem::offset_of!(Uuid, clock_seq_hi_and_reserved) == 8);
const _: () = assert!(core::mem::offset_of!(Uuid, clock_seq_low) == 9);
const _: () = assert!(core::mem::offset_of!(Uuid, node) == 10);

/// `_UUID_NODE_LEN`.
pub const NODE_LEN: usize = 6;

/// The 36-character textual form plus a NUL.
pub const STRING_LEN: usize = 37;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
