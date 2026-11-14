// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `sysinfo`, `get_nprocs*` and `get_*_pages` (`X74`, `11` 3.9, `PG-21`).
//!
//! Before `PG-21` the kernel has no CPU/memory record: `get_nprocs*` answer 1, and `sysinfo`,
//! `get_phys_pages` and `get_avphys_pages` return `ENOSYS` from the seam. The struct is the
//! Linux LP64 layout of `sys/sysinfo.h` (376 bytes); `totalram`/`freeram` are bytes with
//! `mem_unit = 1`, loads and the unused fields are zero.

use crate::seams::{Errno, SysInfoSrc};

/// The Linux `struct sysinfo`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinuxSysinfo {
    /// Seconds since boot.
    pub uptime: u64,
    /// Load averages, fixed-point with `SI_LOAD_SHIFT`.
    pub loads: [u64; 3],
    /// Total usable RAM in `mem_unit` units.
    pub totalram: u64,
    /// Free RAM.
    pub freeram: u64,
    /// Shared RAM.
    pub sharedram: u64,
    /// Buffer RAM.
    pub bufferram: u64,
    /// Total swap.
    pub totalswap: u64,
    /// Free swap.
    pub freeswap: u64,
    /// Number of processes.
    pub procs: u16,
    /// Padding.
    pub pad: u16,
    /// Total high memory.
    pub totalhigh: u64,
    /// Free high memory.
    pub freehigh: u64,
    /// Memory unit size in bytes.
    pub mem_unit: u32,
    /// Reserved.
    pub reserved: [u8; 256],
}

const _: () = assert!(core::mem::size_of::<LinuxSysinfo>() == 368);
const _: () = assert!(core::mem::align_of::<LinuxSysinfo>() == 8);
const _: () = assert!(core::mem::offset_of!(LinuxSysinfo, mem_unit) == 104);

/// `SI_LOAD_SHIFT`.
pub const SI_LOAD_SHIFT: u32 = 16;

/// Fill a `struct sysinfo` from the seam; `page_size` is the runtime page size.
pub fn fill<S: SysInfoSrc>(source: &mut S, page_size: usize) -> Result<LinuxSysinfo, Errno> {
    let info = source.sysinfo()?;
    let page_size = match u64::try_from(page_size) {
        Ok(0) | Err(_) => 4096,
        Ok(value) => value,
    };
    let totalram = info.mem_total_pages.saturating_mul(page_size);
    let freeram = info.mem_free_pages.saturating_mul(page_size);
    Ok(LinuxSysinfo {
        uptime: info.boot_time_ns.checked_div(1_000_000_000).unwrap_or(0),
        loads: [0; 3],
        totalram,
        freeram,
        sharedram: 0,
        bufferram: 0,
        totalswap: 0,
        freeswap: 0,
        procs: 0,
        pad: 0,
        totalhigh: 0,
        freehigh: 0,
        mem_unit: 1,
        reserved: [0; 256],
    })
}

/// `get_nprocs`: one CPU until `PG-21` (or the seam's answer).
pub fn get_nprocs<S: SysInfoSrc>(source: &mut S) -> i32 {
    match source.sysinfo() {
        Ok(info) => i32::try_from(info.cpus_online).unwrap_or(1).max(1),
        Err(_) => 1,
    }
}

/// `get_nprocs_conf`.
pub fn get_nprocs_conf<S: SysInfoSrc>(source: &mut S) -> i32 {
    match source.sysinfo() {
        Ok(info) => i32::try_from(info.cpus_conf).unwrap_or(1).max(1),
        Err(_) => 1,
    }
}

/// `get_phys_pages`.
pub fn get_phys_pages<S: SysInfoSrc>(source: &mut S) -> Result<i64, Errno> {
    let info = source.sysinfo()?;
    i64::try_from(info.mem_total_pages).map_err(|_| Errno::EOVERFLOW)
}

/// `get_avphys_pages`.
pub fn get_avphys_pages<S: SysInfoSrc>(source: &mut S) -> Result<i64, Errno> {
    let info = source.sysinfo()?;
    i64::try_from(info.mem_free_pages).map_err(|_| Errno::EOVERFLOW)
}

#[cfg(test)]
#[path = "sysinfo_ut.rs"]
mod tests;
