// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sysinfo` arithmetic and the `PG-21` defaults.

use super::*;
use crate::testutil::{FakeSysInfo, header_define};

const SYSINFO_H: &str = include_str!("../../../include/sys/sysinfo.h");

#[test]
fn struct_size_matches_the_header_comment() {
    assert_eq!(core::mem::size_of::<LinuxSysinfo>(), 368);
    assert_eq!(core::mem::offset_of!(LinuxSysinfo, loads), 8);
    assert_eq!(core::mem::offset_of!(LinuxSysinfo, totalram), 32);
    assert_eq!(core::mem::offset_of!(LinuxSysinfo, mem_unit), 104);
    assert_eq!(core::mem::offset_of!(LinuxSysinfo, reserved), 108);
}

#[test]
fn header_struct_has_every_field() {
    for name in [
        "uptime",
        "loads",
        "totalram",
        "freeram",
        "sharedram",
        "bufferram",
        "totalswap",
        "freeswap",
        "procs",
        "totalhigh",
        "freehigh",
        "mem_unit",
        "__reserved",
    ] {
        assert!(SYSINFO_H.contains(name), "{name}");
    }
    assert_eq!(header_define(SYSINFO_H, "SI_LOAD_SHIFT"), Some(16));
}

#[test]
fn fill_converts_pages_to_bytes() {
    let mut source = FakeSysInfo::new();
    let info = fill(&mut source, 4096).unwrap();
    assert_eq!(info.totalram, 16384 * 4096);
    assert_eq!(info.freeram, 8192 * 4096);
    assert_eq!(info.mem_unit, 1);
    assert_eq!(info.uptime, 2);
    assert_eq!(info.loads, [0, 0, 0]);
    assert_eq!(info.procs, 0);
}

#[test]
fn fill_returns_enosys_without_pg21() {
    let mut source = FakeSysInfo::without_sysinfo();
    assert_eq!(fill(&mut source, 4096), Err(Errno::ENOSYS));
}

#[test]
fn get_nprocs_is_one_without_pg21() {
    let mut source = FakeSysInfo::without_sysinfo();
    assert_eq!(get_nprocs(&mut source), 1);
    assert_eq!(get_nprocs_conf(&mut source), 1);
}

#[test]
fn get_nprocs_reports_the_record() {
    let mut source = FakeSysInfo::new();
    source.info = Some(crate::seams::RavenSysInfo {
        cpus_conf: 4,
        cpus_online: 2,
        ..crate::seams::RavenSysInfo::default()
    });
    assert_eq!(get_nprocs(&mut source), 2);
    assert_eq!(get_nprocs_conf(&mut source), 4);
}

#[test]
fn pages_are_enosys_without_pg21() {
    let mut source = FakeSysInfo::without_sysinfo();
    assert_eq!(get_phys_pages(&mut source), Err(Errno::ENOSYS));
    assert_eq!(get_avphys_pages(&mut source), Err(Errno::ENOSYS));
}

#[test]
fn pages_report_the_record() {
    let mut source = FakeSysInfo::new();
    assert_eq!(get_phys_pages(&mut source), Ok(16384));
    assert_eq!(get_avphys_pages(&mut source), Ok(8192));
}

#[test]
fn zero_cpu_count_is_clamped_to_one() {
    let mut source = FakeSysInfo::new();
    source.info = Some(crate::seams::RavenSysInfo {
        cpus_conf: 0,
        cpus_online: 0,
        ..crate::seams::RavenSysInfo::default()
    });
    assert_eq!(get_nprocs(&mut source), 1);
    assert_eq!(get_nprocs_conf(&mut source), 1);
}

#[test]
fn page_size_zero_is_replaced_by_default() {
    let mut source = FakeSysInfo::new();
    let info = fill(&mut source, 0).unwrap();
    assert_eq!(info.totalram, 16384 * 4096);
}
