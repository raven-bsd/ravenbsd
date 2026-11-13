// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sysinfo` fallback tests.

use crate::sysinfo::{FALLBACK, SysInfo, get_nprocs, get_phys_pages, sysinfo};

#[test]
fn fallback_is_one_cpu_and_64_mib() {
    assert_eq!(FALLBACK.cpus_conf, 1);
    assert_eq!(FALLBACK.cpus_online, 1);
    assert_eq!(FALLBACK.mem_total_pages, 16384);
    assert_eq!(FALLBACK.mem_free_pages, 8192);
}

#[test]
fn sysinfo_without_arguments_returns_the_fallback() {
    assert_eq!(sysinfo(None, None), FALLBACK);
}

#[test]
fn sysinfo_uses_the_counters_when_present() {
    let info = sysinfo(Some((4, 3)), Some((100, 50)));
    assert_eq!(info.cpus_conf, 4);
    assert_eq!(info.cpus_online, 3);
    assert_eq!(info.mem_total_pages, 100);
    assert_eq!(info.mem_free_pages, 50);
}

#[test]
fn get_nprocs_defaults_to_one() {
    assert_eq!(get_nprocs(None), 1);
    assert_eq!(get_nprocs(Some((8, 6))), 6);
}

#[test]
fn get_phys_pages_defaults_to_16384() {
    assert_eq!(get_phys_pages(None), 16384);
    assert_eq!(get_phys_pages(Some((77, 1))), 77);
}

#[test]
fn sysinfo_is_copy_and_comparable() {
    let info = SysInfo::default();
    let copy = info;
    assert_eq!(info, copy);
}

#[test]
fn getloadavg_is_tbd() {
    assert_eq!(crate::sysinfo::getloadavg(), Err(crate::Errno::ENOSYS));
}
