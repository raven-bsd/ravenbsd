// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The CLEANROOM promise-to-Raven-capability plan.

use crate::promise::{Promise, PromiseSet};

/// VFS permission bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VfsMask(pub u8);

impl VfsMask {
    /// Read.
    pub const READ: u8 = 1;
    /// Write.
    pub const WRITE: u8 = 2;
    /// Create.
    pub const CREATE: u8 = 4;
    /// Device nodes.
    pub const DEVNODE: u8 = 8;
    /// Attribute changes.
    pub const ATTR: u8 = 0x10;
    /// Ownership changes.
    pub const CHOWN: u8 = 0x20;
    /// Advisory locks.
    pub const FLOCK: u8 = 0x40;
}

/// Network permission bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetMask(pub u8);

impl NetMask {
    /// IPv4 sockets.
    pub const INET: u8 = 1;
    /// Local sockets.
    pub const UNIX: u8 = 2;
    /// The DNS handle.
    pub const DNS: u8 = 4;
    /// Multicast.
    pub const MCAST: u8 = 8;
    /// Route reads.
    pub const ROUTE: u8 = 0x10;
    /// Route writes.
    pub const WROUTE: u8 = 0x20;
}

/// Procd operation bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProcdOps(pub u8);

impl ProcdOps {
    /// Fork, kill, wait and process-group operations.
    pub const PROC: u8 = 1;
    /// Exec.
    pub const EXEC: u8 = 2;
    /// Credential changes.
    pub const ID: u8 = 4;
    /// Clock changes.
    pub const SETTIME: u8 = 8;
    /// Process inspection.
    pub const PS: u8 = 0x10;
    /// Virtual-memory information.
    pub const VMINFO: u8 = 0x20;
}

/// Namespace sets to keep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NsKeep(pub u8);

impl NsKeep {
    /// The `file:` namespace.
    pub const FILE: u8 = 1;
    /// The `net:` namespace.
    pub const NET: u8 = 2;
}

/// The capability plan for one promise set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapPlan {
    /// VFS mask.
    pub vfs: VfsMask,
    /// Network mask.
    pub net: NetMask,
    /// Procd operations.
    pub procd: ProcdOps,
    /// Namespace sets to keep.
    pub keep_ns: NsKeep,
    /// Drop the handle-transfer right.
    pub drop_transfer: bool,
    /// Enforcement phase (1, 2 or 3).
    pub phase: u8,
    /// The `/etc/passwd` and `/etc/group` exception.
    pub getpw: bool,
}

/// Build the plan for `set`.
#[must_use]
pub fn plan(set: PromiseSet) -> CapPlan {
    let mut plan = CapPlan {
        drop_transfer: !(set.contains(Promise::Sendfd) && set.contains(Promise::Recvfd)),
        phase: 1,
        ..CapPlan::default()
    };
    let mut phase2 = false;
    let mut phase3 = false;
    if set.contains(Promise::Rpath) {
        plan.vfs.0 |= VfsMask::READ;
        plan.keep_ns.0 |= NsKeep::FILE;
        phase2 = true;
    }
    if set.contains(Promise::Wpath) {
        plan.vfs.0 |= VfsMask::WRITE;
        phase2 = true;
    }
    if set.contains(Promise::Cpath) {
        plan.vfs.0 |= VfsMask::CREATE;
        phase2 = true;
    }
    if set.contains(Promise::Dpath) {
        plan.vfs.0 |= VfsMask::DEVNODE;
        phase2 = true;
    }
    if set.contains(Promise::Fattr) {
        plan.vfs.0 |= VfsMask::ATTR;
        phase2 = true;
    }
    if set.contains(Promise::Chown) {
        plan.vfs.0 |= VfsMask::CHOWN;
        phase2 = true;
    }
    if set.contains(Promise::Flock) {
        plan.vfs.0 |= VfsMask::FLOCK;
        phase2 = true;
    }
    if set.contains(Promise::Inet) {
        plan.net.0 |= NetMask::INET;
        plan.keep_ns.0 |= NsKeep::NET;
        phase2 = true;
    }
    if set.contains(Promise::Unix) {
        plan.net.0 |= NetMask::UNIX;
        phase2 = true;
    }
    if set.contains(Promise::Dns) {
        plan.net.0 |= NetMask::DNS;
        phase2 = true;
    }
    if set.contains(Promise::Mcast) {
        plan.net.0 |= NetMask::MCAST;
        phase3 = true;
    }
    if set.contains(Promise::Route) {
        plan.net.0 |= NetMask::ROUTE;
        phase3 = true;
    }
    if set.contains(Promise::Wroute) {
        plan.net.0 |= NetMask::WROUTE;
        phase3 = true;
    }
    if set.contains(Promise::Proc) {
        plan.procd.0 |= ProcdOps::PROC;
    }
    if set.contains(Promise::Exec) {
        plan.procd.0 |= ProcdOps::EXEC;
    }
    if set.contains(Promise::Id) {
        plan.procd.0 |= ProcdOps::ID;
    }
    if set.contains(Promise::Settime) {
        plan.procd.0 |= ProcdOps::SETTIME;
        phase3 = true;
    }
    if set.contains(Promise::Ps) {
        plan.procd.0 |= ProcdOps::PS;
        phase3 = true;
    }
    if set.contains(Promise::Vminfo) {
        plan.procd.0 |= ProcdOps::VMINFO;
        phase3 = true;
    }
    if set.contains(Promise::Getpw) {
        plan.getpw = true;
        phase2 = true;
    }
    if set.contains(Promise::Sendfd) || set.contains(Promise::Recvfd) {
        phase2 = true;
    }
    if set.contains(Promise::Tty) || set.contains(Promise::Error) {
        phase2 = true;
    }
    if set.contains(Promise::ProtExec)
        || set.contains(Promise::Pf)
        || set.contains(Promise::Bpf)
        || set.contains(Promise::Audio)
        || set.contains(Promise::Video)
        || set.contains(Promise::Drm)
        || set.contains(Promise::Vmm)
        || set.contains(Promise::Tape)
        || set.contains(Promise::Disklabel)
    {
        phase3 = true;
    }
    plan.phase = if phase3 {
        3
    } else if phase2 {
        2
    } else {
        1
    };
    plan
}

#[cfg(test)]
#[path = "capmap_ut.rs"]
mod tests;
