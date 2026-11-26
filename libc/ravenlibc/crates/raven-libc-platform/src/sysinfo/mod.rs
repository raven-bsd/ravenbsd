// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Identity and system information (T8).

pub mod hostname;
pub mod sysctl;
pub mod uname;

pub use hostname::{HOST_NAME_MAX, gethostname, sethostname};
use raven_abi::errno::Errno;
pub use sysctl::{
    MIB_TABLE, ValueKind, sysctl, sysctl_mib_to_name, sysctl_name_to_mib, sysctlbyname,
    sysctlnametomib,
};
pub use uname::uname;

/// The `SYSINFO*` record (PG-21).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SysInfo {
    /// Configured CPUs.
    pub cpus_conf: u32,
    /// Online CPUs.
    pub cpus_online: u32,
    /// Total physical pages.
    pub mem_total_pages: u64,
    /// Free physical pages.
    pub mem_free_pages: u64,
    /// Boot time in nanoseconds since the epoch.
    pub boot_time_ns: u64,
}

/// The fallback used until PG-21 lands: one CPU, 64 MiB total, 32 MiB free.
pub const FALLBACK: SysInfo = SysInfo {
    cpus_conf: 1,
    cpus_online: 1,
    mem_total_pages: (64 * 1024 * 1024) / 4096,
    mem_free_pages: (32 * 1024 * 1024) / 4096,
    boot_time_ns: 0,
};

/// `sysinfo`: fallback values until PG-21, never `-1` (doc 11 section 3.1).
pub fn sysinfo(cpus: Option<(u32, u32)>, pages: Option<(u64, u64)>) -> SysInfo {
    let mut info = FALLBACK;
    if let Some((conf, online)) = cpus {
        info.cpus_conf = conf;
        info.cpus_online = online;
    }
    if let Some((total, free)) = pages {
        info.mem_total_pages = total;
        info.mem_free_pages = free;
    }
    info
}

/// `get_nprocs`/`get_nprocs_conf`.
pub fn get_nprocs(cpus: Option<(u32, u32)>) -> u32 {
    cpus.map_or(FALLBACK.cpus_online, |(_, online)| online)
}

/// `get_nprocs_conf`.
pub fn get_nprocs_conf(cpus: Option<(u32, u32)>) -> u32 {
    cpus.map_or(FALLBACK.cpus_conf, |(conf, _)| conf)
}

/// `get_phys_pages`/`get_avphys_pages`.
pub fn get_phys_pages(pages: Option<(u64, u64)>) -> u64 {
    pages.map_or(FALLBACK.mem_total_pages, |(total, _)| total)
}

/// `get_avphys_pages`.
pub fn get_avphys_pages(pages: Option<(u64, u64)>) -> u64 {
    pages.map_or(FALLBACK.mem_free_pages, |(_, free)| free)
}

/// `getloadavg`: `-1` until PG-27.
pub fn getloadavg() -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

#[cfg(test)]
#[path = "sysinfo_ut.rs"]
mod tests;
