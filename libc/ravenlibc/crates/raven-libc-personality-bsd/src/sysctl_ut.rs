// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The exhaustive `sysctl` table (ticket C66 5.H family `sysctl`).

use super::*;
use crate::testutil::{FakeSysInfo, header_define};
use alloc::vec;

const SYSCTL_H: &str = include_str!("../../../include/sys/sysctl.h");

fn read_name(
    source: &mut FakeSysInfo,
    name: &[u8],
    capacity: usize,
) -> Result<(vec::Vec<u8>, usize), Errno> {
    let mut buffer = vec![0u8; capacity];
    let mut length = capacity;
    sysctlbyname(source, name, Some(&mut buffer), &mut length, None)?;
    buffer.truncate(length);
    Ok((buffer, length))
}

fn read_mib(
    source: &mut FakeSysInfo,
    mib: &[i32],
    capacity: usize,
) -> Result<(vec::Vec<u8>, usize), Errno> {
    let mut buffer = vec![0u8; capacity];
    let mut length = capacity;
    sysctl(source, mib, Some(&mut buffer), &mut length, None)?;
    buffer.truncate(length);
    Ok((buffer, length))
}

#[test]
fn header_numbers_match() {
    for (name, value) in [
        ("CTL_KERN", CTL_KERN),
        ("CTL_HW", CTL_HW),
        ("KERN_OSTYPE", KERN_OSTYPE),
        ("KERN_OSRELEASE", KERN_OSRELEASE),
        ("KERN_VERSION", KERN_VERSION),
        ("KERN_HOSTNAME", KERN_HOSTNAME),
        ("HW_MACHINE", HW_MACHINE),
        ("HW_NCPU", HW_NCPU),
        ("HW_PHYSMEM", HW_PHYSMEM),
        ("HW_PAGESIZE", HW_PAGESIZE),
    ] {
        assert_eq!(
            header_define(SYSCTL_H, name),
            Some(i64::from(value)),
            "{name}"
        );
    }
}

#[test]
fn every_row_reads_by_name() {
    let mut source = FakeSysInfo::new();
    for row in TABLE {
        let result = read_name(&mut source, row.name.as_bytes(), 256);
        assert!(result.is_ok(), "{}: {result:?}", row.name);
        let (_, length) = result.unwrap();
        assert!(length > 0, "{}", row.name);
    }
}

#[test]
fn every_row_reads_by_mib() {
    let mut source = FakeSysInfo::new();
    for row in TABLE {
        let result = read_mib(&mut source, row.mib, 256);
        assert!(result.is_ok(), "{}: {result:?}", row.name);
    }
}

#[test]
fn every_row_name_and_mib_agree() {
    for row in TABLE {
        assert_eq!(
            by_name(row.name.as_bytes()).map(|found| found.mib),
            Some(row.mib)
        );
        assert_eq!(by_mib(row.mib).map(|found| found.name), Some(row.name));
    }
}

#[test]
fn table_has_the_documented_rows() {
    let names: vec::Vec<&str> = TABLE.iter().map(|row| row.name).collect();
    assert_eq!(
        names,
        vec![
            "kern.ostype",
            "kern.osrelease",
            "kern.version",
            "kern.hostname",
            "kern.hostid",
            "kern.domainname",
            "kern.argmax",
            "hw.machine",
            "hw.model",
            "hw.ncpu",
            "hw.physmem",
            "hw.pagesize",
        ]
    );
}

#[test]
fn strings_include_the_nul() {
    let mut source = FakeSysInfo::new();
    let (value, length) = read_name(&mut source, b"kern.ostype", 64).unwrap();
    assert_eq!(value, b"RavenBSD\0");
    assert_eq!(length, 9);
}

#[test]
fn release_and_version_and_machine() {
    let mut source = FakeSysInfo::new();
    assert_eq!(
        read_name(&mut source, b"kern.osrelease", 64).unwrap().0,
        b"0.1\0"
    );
    assert_eq!(
        read_name(&mut source, b"kern.version", 64).unwrap().0,
        b"RavenBSD 0.1\0"
    );
    assert_eq!(
        read_name(&mut source, b"hw.machine", 64).unwrap().0,
        b"x86_64\0"
    );
}

#[test]
fn hostname_value() {
    let mut source = FakeSysInfo::new();
    assert_eq!(
        read_name(&mut source, b"kern.hostname", 64).unwrap().0,
        b"raven\0"
    );
}

#[test]
fn domainname_is_empty() {
    let mut source = FakeSysInfo::new();
    assert_eq!(
        read_name(&mut source, b"kern.domainname", 8).unwrap().0,
        b"\0"
    );
}

#[test]
fn hostid_is_zero_ulong() {
    let mut source = FakeSysInfo::new();
    let (value, length) = read_name(&mut source, b"kern.hostid", 8).unwrap();
    assert_eq!(length, 8);
    assert_eq!(u64::from_ne_bytes(value.try_into().unwrap()), 0);
}

#[test]
fn argmax_is_the_limits_value() {
    let mut source = FakeSysInfo::new();
    let (value, length) = read_name(&mut source, b"kern.argmax", 8).unwrap();
    assert_eq!(length, 8);
    assert_eq!(i64::from_ne_bytes(value.try_into().unwrap()), 131_072);
}

#[test]
fn model_is_ravenbsd_plus_machine() {
    let mut source = FakeSysInfo::new();
    assert_eq!(
        read_name(&mut source, b"hw.model", 64).unwrap().0,
        b"RavenBSD x86_64\0"
    );
}

#[test]
fn pagesize_is_4096() {
    let mut source = FakeSysInfo::new();
    let (value, length) = read_name(&mut source, b"hw.pagesize", 8).unwrap();
    assert_eq!(length, 8);
    assert_eq!(i64::from_ne_bytes(value.try_into().unwrap()), 4096);
}

#[test]
fn ncpu_is_one_before_pg21() {
    let mut source = FakeSysInfo::without_sysinfo();
    let (value, length) = read_name(&mut source, b"hw.ncpu", 8).unwrap();
    assert_eq!(length, 8);
    assert_eq!(i64::from_ne_bytes(value.try_into().unwrap()), 1);
}

#[test]
fn physmem_is_enoent_before_pg21() {
    let mut source = FakeSysInfo::without_sysinfo();
    assert_eq!(read_name(&mut source, b"hw.physmem", 8), Err(Errno::ENOENT));
}

#[test]
fn physmem_is_pages_times_page_size() {
    let mut source = FakeSysInfo::new();
    let (value, length) = read_name(&mut source, b"hw.physmem", 8).unwrap();
    assert_eq!(length, 8);
    assert_eq!(u64::from_ne_bytes(value.try_into().unwrap()), 16384 * 4096);
}

#[test]
fn unknown_name_is_enoent() {
    let mut source = FakeSysInfo::new();
    assert_eq!(
        read_name(&mut source, b"kern.nosuch", 64),
        Err(Errno::ENOENT)
    );
}

#[test]
fn kern_proc_is_enoent() {
    let mut source = FakeSysInfo::new();
    assert_eq!(
        read_name(&mut source, b"kern.proc.pid.1", 64),
        Err(Errno::ENOENT)
    );
}

#[test]
fn unknown_mib_is_enoent() {
    let mut source = FakeSysInfo::new();
    assert_eq!(read_mib(&mut source, &[1, 99], 64), Err(Errno::ENOENT));
}

#[test]
fn old_null_returns_the_needed_length() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    assert_eq!(
        sysctlbyname(&mut source, b"kern.ostype", None, &mut length, None),
        Ok(())
    );
    assert_eq!(length, 9);
}

#[test]
fn short_string_buffer_is_enomem_with_needed_length() {
    let mut source = FakeSysInfo::new();
    let mut buffer = [0u8; 3];
    let mut length = 3usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.ostype",
            Some(&mut buffer),
            &mut length,
            None
        ),
        Err(Errno::ENOMEM)
    );
    assert_eq!(length, 9);
}

#[test]
fn integer_accepts_four_and_eight_bytes() {
    let mut source = FakeSysInfo::new();
    let (four, four_len) = read_name(&mut source, b"hw.ncpu", 4).unwrap();
    assert_eq!(four_len, 4);
    assert_eq!(i32::from_ne_bytes(four.try_into().unwrap()), 1);
    let (eight, eight_len) = read_name(&mut source, b"hw.ncpu", 8).unwrap();
    assert_eq!(eight_len, 8);
    assert_eq!(i64::from_ne_bytes(eight.try_into().unwrap()), 1);
}

#[test]
fn one_or_two_byte_integer_buffer_is_enomem() {
    let mut source = FakeSysInfo::new();
    for capacity in [1usize, 2, 3] {
        let mut buffer = vec![0u8; capacity];
        let mut length = capacity;
        assert_eq!(
            sysctlbyname(
                &mut source,
                b"hw.ncpu",
                Some(&mut buffer),
                &mut length,
                None
            ),
            Err(Errno::ENOMEM)
        );
        assert_eq!(length, 4);
    }
}

#[test]
fn write_to_a_read_only_name_is_eperm() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    assert_eq!(
        sysctlbyname(&mut source, b"kern.ostype", None, &mut length, Some(b"X")),
        Err(Errno::EPERM)
    );
}

#[test]
fn hostname_is_writable() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            None,
            &mut length,
            Some(b"newhost")
        ),
        Ok(())
    );
    assert_eq!(source.hostname, b"newhost");
    assert_eq!(
        read_name(&mut source, b"kern.hostname", 64).unwrap().0,
        b"newhost\0"
    );
}

#[test]
fn hostname_of_64_bytes_is_accepted_and_65_is_einval() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    let name = vec![b'a'; 64];
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            None,
            &mut length,
            Some(&name)
        ),
        Ok(())
    );
    let long = vec![b'a'; 65];
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            None,
            &mut length,
            Some(&long)
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn sysctlnametomib_roundtrip() {
    for row in TABLE {
        let mut mib = [0i32; 4];
        let mut length = mib.len();
        assert_eq!(
            sysctlnametomib(row.name.as_bytes(), &mut mib, &mut length),
            Ok(())
        );
        assert_eq!(length, row.mib.len());
        assert_eq!(&mib[..length], row.mib);
    }
}

#[test]
fn sysctlnametomib_unknown_is_enoent() {
    let mut mib = [0i32; 4];
    let mut length = 4usize;
    assert_eq!(
        sysctlnametomib(b"nope", &mut mib, &mut length),
        Err(Errno::ENOENT)
    );
}

#[test]
fn sysctlnametomib_small_buffer_is_enomem() {
    let mut mib = [0i32; 1];
    let mut length = 1usize;
    assert_eq!(
        sysctlnametomib(b"kern.hostname", &mut mib, &mut length),
        Err(Errno::ENOMEM)
    );
    assert_eq!(length, 2);
}

#[test]
fn sysctlgetmibinfo_matches_nametomib() {
    let mut mib = [0i32; 4];
    let mut length = 4usize;
    assert_eq!(
        sysctlgetmibinfo(b"hw.pagesize", &mut mib, &mut length),
        Ok(())
    );
    assert_eq!(length, 2);
    assert_eq!(mib[0], CTL_HW);
    assert_eq!(mib[1], HW_PAGESIZE);
}
