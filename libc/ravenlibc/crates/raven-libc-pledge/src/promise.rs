// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The OpenBSD promise names and their parser.

use crate::errors::{EINVAL, ENAMETOOLONG, Errno};
use crate::limits::MAX_PLEDGE_STRING;

/// One promise, in `pledgenames[]` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Promise {
    /// `stdio`.
    Stdio = 0,
    /// `rpath`.
    Rpath = 1,
    /// `wpath`.
    Wpath = 2,
    /// `cpath`.
    Cpath = 3,
    /// `dpath`.
    Dpath = 4,
    /// `inet`.
    Inet = 5,
    /// `mcast`.
    Mcast = 6,
    /// `fattr`.
    Fattr = 7,
    /// `chown`.
    Chown = 8,
    /// `flock`.
    Flock = 9,
    /// `unix`.
    Unix = 10,
    /// `dns`.
    Dns = 11,
    /// `getpw`.
    Getpw = 12,
    /// `sendfd`.
    Sendfd = 13,
    /// `recvfd`.
    Recvfd = 14,
    /// `tape`.
    Tape = 15,
    /// `tty`.
    Tty = 16,
    /// `proc`.
    Proc = 17,
    /// `exec`.
    Exec = 18,
    /// `prot_exec`.
    ProtExec = 19,
    /// `settime`.
    Settime = 20,
    /// `ps`.
    Ps = 21,
    /// `vminfo`.
    Vminfo = 22,
    /// `id`.
    Id = 23,
    /// `pf`.
    Pf = 24,
    /// `route`.
    Route = 25,
    /// `wroute`.
    Wroute = 26,
    /// `audio`.
    Audio = 27,
    /// `video`.
    Video = 28,
    /// `bpf`.
    Bpf = 29,
    /// `unveil`.
    Unveil = 30,
    /// `error`.
    Error = 31,
    /// `disklabel`.
    Disklabel = 32,
    /// `drm`.
    Drm = 33,
    /// `vmm`.
    Vmm = 34,
}

impl Promise {
    /// Every promise in donor order.
    pub const ALL: [Promise; 35] = [
        Promise::Stdio,
        Promise::Rpath,
        Promise::Wpath,
        Promise::Cpath,
        Promise::Dpath,
        Promise::Inet,
        Promise::Mcast,
        Promise::Fattr,
        Promise::Chown,
        Promise::Flock,
        Promise::Unix,
        Promise::Dns,
        Promise::Getpw,
        Promise::Sendfd,
        Promise::Recvfd,
        Promise::Tape,
        Promise::Tty,
        Promise::Proc,
        Promise::Exec,
        Promise::ProtExec,
        Promise::Settime,
        Promise::Ps,
        Promise::Vminfo,
        Promise::Id,
        Promise::Pf,
        Promise::Route,
        Promise::Wroute,
        Promise::Audio,
        Promise::Video,
        Promise::Bpf,
        Promise::Unveil,
        Promise::Error,
        Promise::Disklabel,
        Promise::Drm,
        Promise::Vmm,
    ];

    /// The donor name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Rpath => "rpath",
            Self::Wpath => "wpath",
            Self::Cpath => "cpath",
            Self::Dpath => "dpath",
            Self::Inet => "inet",
            Self::Mcast => "mcast",
            Self::Fattr => "fattr",
            Self::Chown => "chown",
            Self::Flock => "flock",
            Self::Unix => "unix",
            Self::Dns => "dns",
            Self::Getpw => "getpw",
            Self::Sendfd => "sendfd",
            Self::Recvfd => "recvfd",
            Self::Tape => "tape",
            Self::Tty => "tty",
            Self::Proc => "proc",
            Self::Exec => "exec",
            Self::ProtExec => "prot_exec",
            Self::Settime => "settime",
            Self::Ps => "ps",
            Self::Vminfo => "vminfo",
            Self::Id => "id",
            Self::Pf => "pf",
            Self::Route => "route",
            Self::Wroute => "wroute",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Bpf => "bpf",
            Self::Unveil => "unveil",
            Self::Error => "error",
            Self::Disklabel => "disklabel",
            Self::Drm => "drm",
            Self::Vmm => "vmm",
        }
    }

    /// The bit index of this promise.
    #[must_use]
    pub const fn index(self) -> u32 {
        self as u32
    }

    /// Parse one name.
    #[must_use]
    pub fn from_name(name: &[u8]) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|promise| promise.name().as_bytes() == name)
    }
}

/// A bit set of promises; bit `i` is `Promise::ALL[i]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromiseSet(pub u64);

impl PromiseSet {
    /// The empty set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Whether `promise` is present.
    #[must_use]
    pub const fn contains(self, promise: Promise) -> bool {
        self.0 & (1u64 << promise.index()) != 0
    }

    /// Whether any bit of `other` is present.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// The union.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The intersection.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Remove every bit of `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Whether no promise is present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Parse an OpenBSD promise string.
pub fn parse_promises(text: &[u8]) -> Result<PromiseSet, Errno> {
    if text.len() > MAX_PLEDGE_STRING {
        return Err(ENAMETOOLONG);
    }
    if text.is_empty() {
        return Ok(PromiseSet(0));
    }
    let mut set = 0u64;
    let mut first = true;
    for token in text.split(|byte| *byte == b' ') {
        if token.is_empty() {
            if first {
                return Err(EINVAL);
            }
            continue;
        }
        let promise = Promise::from_name(token).ok_or(EINVAL)?;
        set |= 1u64 << promise.index();
        first = false;
    }
    Ok(PromiseSet(set))
}

#[cfg(test)]
#[path = "promise_ut.rs"]
mod tests;
