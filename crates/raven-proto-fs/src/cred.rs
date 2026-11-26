// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Caller credentials and the owner/group/other permission matrix.
//!
//! The scheme packet carries the effective caller snapshot; the filesystem servers use this
//! value for every access decision. The kernel remains the authority for the ids and for
//! setuid/setgid exec (U3b).

/// Maximum supplementary groups carried by one scheme request.
pub const MAX_GROUPS: usize = 16;

/// Owner/group/other read bit.
pub const ACCESS_READ: u16 = 0o4;
/// Owner/group/other write bit.
pub const ACCESS_WRITE: u16 = 0o2;
/// Owner/group/other execute bit.
pub const ACCESS_EXEC: u16 = 0o1;

/// One caller credential snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Credentials {
    /// Real user id.
    pub uid: u32,
    /// Effective user id (the id used for access checks).
    pub euid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id (the id used for access checks).
    pub egid: u32,
    /// Number of valid entries in [`Self::groups`].
    pub group_count: u32,
    /// Supplementary groups.
    pub groups: [u32; MAX_GROUPS],
    /// File-creation mask recorded by the kernel when the request was submitted.
    pub umask: u32,
}

impl Credentials {
    /// The kernel/root credential snapshot.
    pub const ROOT: Self = Self {
        uid: 0,
        euid: 0,
        gid: 0,
        egid: 0,
        group_count: 0,
        groups: [0; MAX_GROUPS],
        umask: 0,
    };

    /// Build a snapshot, clamping the group count to the array bound.
    #[must_use]
    pub const fn new(
        uid: u32,
        euid: u32,
        gid: u32,
        egid: u32,
        groups: [u32; MAX_GROUPS],
        group_count: u32,
        umask: u32,
    ) -> Self {
        Self {
            uid,
            euid,
            gid,
            egid,
            group_count: if group_count > MAX_GROUPS as u32 {
                MAX_GROUPS as u32
            } else {
                group_count
            },
            groups,
            umask,
        }
    }

    /// True for the privileged effective uid.
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.euid == 0
    }

    /// The valid supplementary group slice.
    #[must_use]
    pub fn group_list(&self) -> &[u32] {
        let count = self.group_count as usize;
        self.groups.get(..count).unwrap_or(&[])
    }

    /// True when `gid` is the effective group or a supplementary group.
    #[must_use]
    pub fn in_group(&self, gid: u32) -> bool {
        self.egid == gid || self.group_list().contains(&gid)
    }

    /// The permission bits selected for this caller: owner, group or other.
    #[must_use]
    pub fn class_bits(&self, mode: u16, owner: u32, group: u32) -> u16 {
        if self.euid == owner {
            (mode >> 6) & 0o7
        } else if self.in_group(group) {
            (mode >> 3) & 0o7
        } else {
            mode & 0o7
        }
    }

    /// True when the caller may perform `want` (`ACCESS_READ|WRITE|EXEC`) on a node.
    ///
    /// Root bypasses read and write, but still needs at least one execute bit for `EXEC`.
    #[must_use]
    pub fn permitted(&self, mode: u16, owner: u32, group: u32, want: u16) -> bool {
        if self.is_root() {
            return want & ACCESS_EXEC == 0 || mode & 0o111 != 0;
        }
        self.class_bits(mode, owner, group) & want == want
    }

    /// True when the caller may execute a file with this mode.
    #[must_use]
    pub fn can_exec(&self, mode: u16, owner: u32, group: u32) -> bool {
        self.permitted(mode, owner, group, ACCESS_EXEC)
    }

    /// True when the caller may remove/rename `file_owner`'s entry from `dir_mode`.
    ///
    /// A sticky directory limits deletion and rename to the file owner or root; a non-sticky
    /// directory leaves the parent write/execute check to the caller.
    #[must_use]
    pub fn sticky_delete_allowed(&self, dir_mode: u16, file_owner: u32) -> bool {
        dir_mode & 0o1000 == 0 || self.is_root() || self.euid == file_owner
    }
}

#[cfg(test)]
#[path = "cred_ut.rs"]
mod tests;
