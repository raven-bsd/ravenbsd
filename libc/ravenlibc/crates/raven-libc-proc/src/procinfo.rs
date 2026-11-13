// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The read-only procinfo page and its seqlock reader.

use crate::errors::{EAGAIN, EIO, EPROTO, Errno};

/// Size of the procinfo page.
pub const PAGE_LEN: usize = 4096;
/// Magic value `RPI1`.
pub const MAGIC: u32 = 0x5250_4931;

/// Offset of the magic field.
pub const OFF_MAGIC: usize = 0;
/// Offset of the version field.
pub const OFF_VERSION: usize = 4;
/// Offset of the sequence counter.
pub const OFF_SEQ: usize = 8;
/// Offset of the pid field.
pub const OFF_PID: usize = 16;
/// Offset of the ppid field.
pub const OFF_PPID: usize = 20;
/// Offset of the pgid field.
pub const OFF_PGID: usize = 24;
/// Offset of the sid field.
pub const OFF_SID: usize = 28;
/// Offset of the six credential fields.
pub const OFF_IDS: usize = 32;
/// Offset of the umask field.
pub const OFF_UMASK: usize = 56;
/// Offset of the nice field.
pub const OFF_NICE: usize = 60;
/// Offset of the group count.
pub const OFF_NGROUPS: usize = 64;
/// Offset of the flags field.
pub const OFF_FLAGS: usize = 68;
/// Offset of the group array.
pub const OFF_GROUPS: usize = 72;
/// Offset of the 16 rlimit pairs.
pub const OFF_RLIMITS: usize = 200;
/// Offset of the login name.
pub const OFF_LOGIN: usize = 456;
/// Offset of the title.
pub const OFF_TITLE: usize = 489;

/// A page with a sequence counter and bounded reads.
pub trait SeqPage {
    /// Read the seqlock counter.
    fn seq(&self) -> u32;
    /// Copy `out.len()` bytes at `off`; false when the range is unreadable.
    fn read(&self, off: usize, out: &mut [u8]) -> bool;
}

/// One decoded procinfo record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    /// Page format version.
    pub version: u32,
    /// Sequence counter observed in the stable snapshot.
    pub seq: u32,
    /// Process id.
    pub pid: i32,
    /// Parent process id.
    pub ppid: i32,
    /// Process group id.
    pub pgid: i32,
    /// Session id.
    pub sid: i32,
    /// Real, effective and saved user ids.
    pub ruid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Saved user id.
    pub suid: u32,
    /// Real group id.
    pub rgid: u32,
    /// Effective group id.
    pub egid: u32,
    /// Saved group id.
    pub sgid: u32,
    /// File-creation mask.
    pub umask: u32,
    /// Nice value.
    pub nice: i32,
    /// Number of valid groups, capped at 32.
    pub ngroups: u32,
    /// Process flags.
    pub flags: u32,
    /// First 32 supplementary groups.
    pub groups: [u32; 32],
    /// Whether the page held more than 32 groups.
    pub more_groups: bool,
    /// The 16 resource-limit pairs.
    pub rlimits: [[u64; 2]; 16],
    /// Login name, NUL padded.
    pub login: [u8; 33],
    /// Process title, NUL padded.
    pub title: [u8; 256],
}

/// Read a stable procinfo snapshot, retrying a changing seqlock.
pub fn read_procinfo(page: &dyn SeqPage) -> Result<ProcInfo, Errno> {
    if read_u32(page, OFF_MAGIC)? != MAGIC {
        return Err(EPROTO);
    }
    for _ in 0..1000u32 {
        let seq = page.seq();
        if seq & 1 != 0 {
            continue;
        }
        let version = read_u32(page, OFF_VERSION)?;
        let pid = read_i32(page, OFF_PID)?;
        let ppid = read_i32(page, OFF_PPID)?;
        let pgid = read_i32(page, OFF_PGID)?;
        let sid = read_i32(page, OFF_SID)?;
        let ruid = read_u32(page, OFF_IDS)?;
        let euid = read_u32(page, OFF_IDS.saturating_add(4))?;
        let suid = read_u32(page, OFF_IDS.saturating_add(8))?;
        let rgid = read_u32(page, OFF_IDS.saturating_add(12))?;
        let egid = read_u32(page, OFF_IDS.saturating_add(16))?;
        let sgid = read_u32(page, OFF_IDS.saturating_add(20))?;
        let umask = read_u32(page, OFF_UMASK)?;
        let nice = read_i32(page, OFF_NICE)?;
        let ngroups = read_u32(page, OFF_NGROUPS)?;
        let flags = read_u32(page, OFF_FLAGS)?;
        let mut groups = [0u32; 32];
        for (index, slot) in groups.iter_mut().enumerate() {
            let offset = OFF_GROUPS.saturating_add(index.saturating_mul(4));
            *slot = read_u32(page, offset)?;
        }
        let mut rlimits = [[0u64; 2]; 16];
        for (index, pair) in rlimits.iter_mut().enumerate() {
            let offset = OFF_RLIMITS.saturating_add(index.saturating_mul(16));
            pair[0] = read_u64(page, offset)?;
            pair[1] = read_u64(page, offset.saturating_add(8))?;
        }
        let mut login = [0u8; 33];
        read_bytes(page, OFF_LOGIN, &mut login)?;
        let mut title = [0u8; 256];
        read_bytes(page, OFF_TITLE, &mut title)?;
        if page.seq() != seq {
            continue;
        }
        return Ok(ProcInfo {
            version,
            seq,
            pid,
            ppid,
            pgid,
            sid,
            ruid,
            euid,
            suid,
            rgid,
            egid,
            sgid,
            umask,
            nice,
            ngroups: ngroups.min(32),
            flags,
            groups,
            more_groups: ngroups > 32,
            rlimits,
            login,
            title,
        });
    }
    Err(EAGAIN)
}

fn read_bytes(page: &dyn SeqPage, offset: usize, out: &mut [u8]) -> Result<(), Errno> {
    if page.read(offset, out) {
        Ok(())
    } else {
        Err(EIO)
    }
}

fn read_u32(page: &dyn SeqPage, offset: usize) -> Result<u32, Errno> {
    let mut bytes = [0u8; 4];
    read_bytes(page, offset, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_i32(page: &dyn SeqPage, offset: usize) -> Result<i32, Errno> {
    let mut bytes = [0u8; 4];
    read_bytes(page, offset, &mut bytes)?;
    Ok(i32::from_le_bytes(bytes))
}

fn read_u64(page: &dyn SeqPage, offset: usize) -> Result<u64, Errno> {
    let mut bytes = [0u8; 8];
    read_bytes(page, offset, &mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
#[path = "procinfo_ut.rs"]
mod tests;
