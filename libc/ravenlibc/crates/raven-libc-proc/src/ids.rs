// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Credential-change argument builders and validation.

use alloc::vec::Vec;

use crate::errors::{EINVAL, Errno};
use crate::limits::NGROUPS_MAX;

/// One `SETCRED` request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetCred {
    /// Kind: 1 setuid, 2 seteuid, 3 setreuid, 4 setresuid, 5 setgid, 6 setegid,
    /// 7 setregid, 8 setresgid, 9 setgroups.
    pub kind: u32,
    /// First argument.
    pub a: i32,
    /// Second argument.
    pub b: i32,
    /// Third argument.
    pub c: i32,
    /// Supplementary groups for kind 9.
    pub groups: Vec<u32>,
}

impl SetCred {
    fn empty(kind: u32) -> Self {
        Self {
            kind,
            a: -1,
            b: -1,
            c: -1,
            groups: Vec::new(),
        }
    }

    /// `setuid`.
    #[must_use]
    pub fn setuid(uid: u32) -> Self {
        let mut value = Self::empty(1);
        value.a = uid as i32;
        value
    }

    /// `seteuid`.
    #[must_use]
    pub fn seteuid(uid: u32) -> Self {
        let mut value = Self::empty(2);
        value.a = uid as i32;
        value
    }

    /// `setreuid`; `-1` leaves the corresponding id unchanged.
    #[must_use]
    pub fn setreuid(real: i32, effective: i32) -> Self {
        let mut value = Self::empty(3);
        value.a = real;
        value.b = effective;
        value
    }

    /// `setresuid`; `-1` leaves the corresponding id unchanged.
    #[must_use]
    pub fn setresuid(real: i32, effective: i32, saved: i32) -> Self {
        let mut value = Self::empty(4);
        value.a = real;
        value.b = effective;
        value.c = saved;
        value
    }

    /// `setgid`.
    #[must_use]
    pub fn setgid(gid: u32) -> Self {
        let mut value = Self::empty(5);
        value.a = gid as i32;
        value
    }

    /// `setegid`.
    #[must_use]
    pub fn setegid(gid: u32) -> Self {
        let mut value = Self::empty(6);
        value.a = gid as i32;
        value
    }

    /// `setregid`; `-1` leaves the corresponding id unchanged.
    #[must_use]
    pub fn setregid(real: i32, effective: i32) -> Self {
        let mut value = Self::empty(7);
        value.a = real;
        value.b = effective;
        value
    }

    /// `setresgid`; `-1` leaves the corresponding id unchanged.
    #[must_use]
    pub fn setresgid(real: i32, effective: i32, saved: i32) -> Self {
        let mut value = Self::empty(8);
        value.a = real;
        value.b = effective;
        value.c = saved;
        value
    }

    /// `setgroups`.
    pub fn setgroups(groups: &[u32]) -> Result<Self, Errno> {
        if groups.len() > NGROUPS_MAX {
            return Err(EINVAL);
        }
        let mut value = Self::empty(9);
        value.groups.extend_from_slice(groups);
        Ok(value)
    }

    /// Validate the request before it is sent.
    pub fn validate(&self) -> Result<(), Errno> {
        if !(1..=9).contains(&self.kind) {
            return Err(EINVAL);
        }
        if self.kind == 9 && self.groups.len() > NGROUPS_MAX {
            return Err(EINVAL);
        }
        Ok(())
    }
}

/// `issetugid()` is always false on Raven (H-7, O-7).
#[must_use]
pub const fn issetugid() -> bool {
    false
}

#[cfg(test)]
#[path = "ids_ut.rs"]
mod tests;
