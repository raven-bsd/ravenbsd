// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The exhaustive `sysctl` table (ticket C66 5.H family `sysctl`).
//!
//! The expected values come from the pinned FreeBSD `133344405f` sources:
//! `sys/sys/sysctl.h` for the numbers and `sys/kern/kern_sysctl.c`,
//! `sys/kern/kern_mib.c` for the encodings and the error/partial-copy rules.

use super::*;
use crate::seams::{RavenSysInfo, SysInfoSrc, Uname};
use crate::testutil::{FakeSysInfo, header_define};
use alloc::vec;

const SYSCTL_H: &str = include_str!("../../../include/sys/sysctl.h");

fn read_name<S: SysInfoSrc>(
    source: &mut S,
    name: &[u8],
    capacity: usize,
) -> Result<(vec::Vec<u8>, usize), Errno> {
    let mut buffer = vec![0u8; capacity];
    let mut length = capacity;
    sysctlbyname(source, name, Some(&mut buffer), &mut length, None)?;
    buffer.truncate(length);
    Ok((buffer, length))
}

fn read_mib<S: SysInfoSrc>(
    source: &mut S,
    mib: &[i32],
    capacity: usize,
) -> Result<(vec::Vec<u8>, usize), Errno> {
    let mut buffer = vec![0u8; capacity];
    let mut length = capacity;
    sysctl(source, mib, Some(&mut buffer), &mut length, None)?;
    buffer.truncate(length);
    Ok((buffer, length))
}

/// A source whose hostname provider accepts any length and can fail on demand.
struct ScriptedSource {
    base: FakeSysInfo,
    error: Option<Errno>,
    calls: usize,
}

impl ScriptedSource {
    fn new(error: Option<Errno>) -> Self {
        Self {
            base: FakeSysInfo::new(),
            error,
            calls: 0,
        }
    }
}

impl SysInfoSrc for ScriptedSource {
    fn sysinfo(&mut self) -> Result<RavenSysInfo, Errno> {
        self.base.sysinfo()
    }

    fn uname(&mut self) -> Result<Uname, Errno> {
        self.base.uname()
    }

    fn hostname(&mut self) -> Result<vec::Vec<u8>, Errno> {
        Ok(self.base.hostname.clone())
    }

    fn set_hostname(&mut self, name: &[u8]) -> Result<(), Errno> {
        self.calls = self.calls.saturating_add(1);
        if let Some(error) = self.error {
            return Err(error);
        }
        self.base.hostname = name.to_vec();
        self.base.set_names.push(name.to_vec());
        Ok(())
    }
}

#[test]
fn header_numbers_match() {
    for (name, value) in [
        ("CTL_MAXNAME", 24),
        ("CTL_KERN", CTL_KERN),
        ("CTL_HW", CTL_HW),
        ("KERN_OSTYPE", KERN_OSTYPE),
        ("KERN_OSRELEASE", KERN_OSRELEASE),
        ("KERN_VERSION", KERN_VERSION),
        ("KERN_ARGMAX", 8),
        ("KERN_HOSTNAME", KERN_HOSTNAME),
        ("KERN_HOSTID", 11),
        ("KERN_NISDOMAINNAME", 22),
        ("HW_MACHINE", HW_MACHINE),
        ("HW_MODEL", 2),
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
fn argmax_and_model_use_the_freebsd_numbers() {
    let argmax = by_name(b"kern.argmax").expect("kern.argmax");
    assert_eq!(argmax.mib, &[CTL_KERN, 8][..]);
    let model = by_name(b"hw.model").expect("hw.model");
    assert_eq!(model.mib, &[CTL_HW, 2][..]);
}

#[test]
fn every_supported_row_reads_by_name() {
    let mut source = FakeSysInfo::new();
    for row in TABLE {
        let result = read_name(&mut source, row.name.as_bytes(), 256);
        if row.name == "hw.model" {
            assert_eq!(result, Err(Errno::ENOENT), "{}", row.name);
        } else {
            let (_, length) = result.unwrap();
            assert!(length > 0, "{}", row.name);
        }
    }
}

#[test]
fn every_supported_row_reads_by_mib() {
    let mut source = FakeSysInfo::new();
    for row in TABLE {
        let result = read_mib(&mut source, row.mib, 256);
        if row.name == "hw.model" {
            assert_eq!(result, Err(Errno::ENOENT), "{}", row.name);
        } else {
            assert!(result.is_ok(), "{}: {result:?}", row.name);
        }
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
fn size_query_reports_the_declared_width_for_every_row() {
    let mut source = FakeSysInfo::new();
    for row in TABLE {
        let mut length = 0usize;
        let result = sysctlbyname(&mut source, row.name.as_bytes(), None, &mut length, None);
        if row.name == "hw.model" {
            assert_eq!(result, Err(Errno::ENOENT), "{}", row.name);
            continue;
        }
        assert_eq!(result, Ok(()), "{}", row.name);
        match row.kind {
            ValueKind::Int => assert_eq!(length, 4, "{}", row.name),
            ValueKind::ULong => assert_eq!(length, 8, "{}", row.name),
            ValueKind::Text => assert!(length > 0, "{}", row.name),
        }
        let (_, actual) = read_name(&mut source, row.name.as_bytes(), length).unwrap();
        assert_eq!(actual, length, "{}", row.name);
    }
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
    assert_eq!(length, 4);
    assert_eq!(
        i32::from_ne_bytes(value.try_into().unwrap()),
        raven_libc_abi::limits::ARG_MAX
    );
}

#[test]
fn model_has_no_authoritative_provider() {
    let mut source = FakeSysInfo::new();
    assert_eq!(read_name(&mut source, b"hw.model", 64), Err(Errno::ENOENT));
}

#[test]
fn pagesize_is_4096() {
    let mut source = FakeSysInfo::new();
    let (value, length) = read_name(&mut source, b"hw.pagesize", 8).unwrap();
    assert_eq!(length, 4);
    assert_eq!(i32::from_ne_bytes(value.try_into().unwrap()), 4096);
}

#[test]
fn ncpu_without_sysinfo_is_unsupported() {
    let mut source = FakeSysInfo::without_sysinfo();
    assert_eq!(read_name(&mut source, b"hw.ncpu", 8), Err(Errno::ENOENT));
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
fn malformed_mib_lengths_are_einval() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    assert_eq!(
        sysctl(&mut source, &[], None, &mut length, None),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        sysctl(&mut source, &[CTL_HW], None, &mut length, None),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        sysctl(&mut source, &[CTL_HW; 25], None, &mut length, None),
        Err(Errno::EINVAL)
    );
}

#[test]
fn empty_and_overlong_names_are_einval() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    assert_eq!(
        sysctlbyname(&mut source, b"", None, &mut length, None),
        Err(Errno::EINVAL)
    );
    let overlong = vec![b'a'; raven_libc_abi::limits::PATH_MAX as usize + 1];
    assert_eq!(
        sysctlbyname(&mut source, &overlong, None, &mut length, None),
        Err(Errno::EINVAL)
    );
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
fn short_string_buffer_reports_the_copied_bytes() {
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
    assert_eq!(length, 3);
    assert_eq!(&buffer, b"Rav");
}

#[test]
fn integer_values_are_four_bytes_even_in_a_larger_buffer() {
    let mut source = FakeSysInfo::new();
    for (name, expected) in [
        ("kern.argmax", raven_libc_abi::limits::ARG_MAX),
        ("hw.ncpu", 1),
        ("hw.pagesize", 4096),
    ] {
        let mut buffer = [0u8; 8];
        let mut length = 8usize;
        assert_eq!(
            sysctlbyname(
                &mut source,
                name.as_bytes(),
                Some(&mut buffer),
                &mut length,
                None
            ),
            Ok(()),
            "{name}"
        );
        assert_eq!(length, 4, "{name}");
        assert_eq!(
            i32::from_ne_bytes(buffer[..4].try_into().unwrap()),
            expected,
            "{name}"
        );
    }
}

#[test]
fn ulong_value_does_not_narrow_to_four_bytes() {
    let mut source = FakeSysInfo::new();
    let mut buffer = [0u8; 4];
    let mut length = 4usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"hw.physmem",
            Some(&mut buffer),
            &mut length,
            None
        ),
        Err(Errno::ENOMEM)
    );
    assert_eq!(length, 4);
    let expected = (16384u64 * 4096).to_ne_bytes();
    assert_eq!(&buffer, &expected[..4]);
}

#[test]
fn short_integer_buffer_reports_the_copied_bytes() {
    let mut source = FakeSysInfo::new();
    let expected = raven_libc_abi::limits::ARG_MAX.to_ne_bytes();
    for capacity in [1usize, 2, 3] {
        let mut buffer = [0u8; 3];
        let mut length = capacity;
        assert_eq!(
            sysctlbyname(
                &mut source,
                b"kern.argmax",
                Some(&mut buffer[..capacity]),
                &mut length,
                None
            ),
            Err(Errno::ENOMEM)
        );
        assert_eq!(length, capacity, "capacity {capacity}");
        assert_eq!(&buffer[..capacity], &expected[..capacity]);
    }
}

#[test]
fn write_to_a_read_only_name_is_eperm() {
    let mut source = FakeSysInfo::new();
    let mut buffer = [0xAAu8; 16];
    let mut length = 16usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.ostype",
            Some(&mut buffer),
            &mut length,
            Some(b"X")
        ),
        Err(Errno::EPERM)
    );
    assert_eq!(length, 16);
    assert!(buffer.iter().all(|byte| *byte == 0xAA));
}

#[test]
fn write_does_not_happen_when_the_old_buffer_is_too_small() {
    let mut source = FakeSysInfo::new();
    let mut buffer = [0u8; 3];
    let mut length = 3usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            Some(&mut buffer),
            &mut length,
            Some(b"next")
        ),
        Err(Errno::ENOMEM)
    );
    assert_eq!(length, 3);
    assert_eq!(&buffer, b"rav");
    assert_eq!(source.hostname, b"raven");
}

#[test]
fn write_reports_the_old_value_before_the_new_value() {
    let mut source = FakeSysInfo::new();
    let mut buffer = [0u8; 64];
    let mut length = 64usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            Some(&mut buffer),
            &mut length,
            Some(b"next")
        ),
        Ok(())
    );
    assert_eq!(&buffer[..length], b"raven\0");
    assert_eq!(source.hostname, b"next");
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
fn repeated_reads_return_the_written_value() {
    let mut source = FakeSysInfo::new();
    let mut length = 0usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            None,
            &mut length,
            Some(b"next")
        ),
        Ok(())
    );
    assert_eq!(
        read_name(&mut source, b"kern.hostname", 64).unwrap().0,
        b"next\0"
    );
    assert_eq!(
        read_mib(&mut source, &[CTL_KERN, KERN_HOSTNAME], 64)
            .unwrap()
            .0,
        b"next\0"
    );
}

#[test]
fn hostname_write_limit_is_the_freebsd_maxhostnamelen() {
    let mut source = ScriptedSource::new(None);
    let mut length = 0usize;
    let name = vec![b'a'; 255];
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
    assert_eq!(source.calls, 1);
    let too_long = vec![b'a'; 256];
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            None,
            &mut length,
            Some(&too_long)
        ),
        Err(Errno::EINVAL)
    );
    assert_eq!(source.calls, 1);
}

#[test]
fn hostname_provider_error_is_returned() {
    let mut source = ScriptedSource::new(Some(Errno::EPERM));
    let mut length = 0usize;
    assert_eq!(
        sysctlbyname(
            &mut source,
            b"kern.hostname",
            None,
            &mut length,
            Some(b"next")
        ),
        Err(Errno::EPERM)
    );
    assert_eq!(source.calls, 1);
    assert_eq!(source.base.hostname, b"raven");
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
fn sysctlnametomib_small_buffer_keeps_the_caller_length() {
    let mut mib = [0i32; 1];
    let mut length = 1usize;
    assert_eq!(
        sysctlnametomib(b"kern.hostname", &mut mib, &mut length),
        Err(Errno::ENOMEM)
    );
    assert_eq!(length, 1);
}

#[test]
fn sysctlnametomib_name_rules() {
    let mut mib = [0i32; 4];
    let mut length = 4usize;
    assert_eq!(
        sysctlnametomib(b"", &mut mib, &mut length),
        Err(Errno::EINVAL)
    );
    let overlong = vec![b'a'; raven_libc_abi::limits::PATH_MAX as usize + 1];
    assert_eq!(
        sysctlnametomib(&overlong, &mut mib, &mut length),
        Err(Errno::EINVAL)
    );
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
