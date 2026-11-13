// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sysctl` shim tests.

use raven_abi::errno::Errno;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::sysinfo::sysctl::{
    CTL_HW, CTL_KERN, HW_NCPU, KERN_HOSTNAME, sysctl, sysctl_mib_to_name, sysctl_name_to_mib,
    sysctlbyname, sysctlnametomib,
};

fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.sys(b"kern/hostname", b"raven\n");
    fake.sys(b"kern/ostype", b"RavenBSD\n");
    fake.sys(b"hw/ncpu", b"4\n");
    fake.sys(b"hw/pagesize", b"4096\n");
    Platform::new(fake)
}

#[test]
fn name_to_mib() {
    let entry = sysctl_name_to_mib(b"kern.hostname").expect("known");
    assert_eq!(entry.mib, &[CTL_KERN, KERN_HOSTNAME]);
    assert_eq!(entry.name, "kern.hostname");
}

#[test]
fn mib_to_name() {
    let entry = sysctl_mib_to_name(&[CTL_HW, HW_NCPU]).expect("known");
    assert_eq!(entry.name, "hw.ncpu");
}

#[test]
fn unknown_name_is_enoent() {
    assert_eq!(sysctl_name_to_mib(b"kern.nope"), Err(Errno::ENOENT));
    assert_eq!(sysctl_mib_to_name(&[99, 99]), Err(Errno::ENOENT));
}

#[test]
fn sysctlnametomib_copies_the_path() {
    let mut out = [0i32; 4];
    let length = sysctlnametomib(b"kern.hostname", &mut out).expect("mib");
    assert_eq!(length, 2);
    assert_eq!(&out[..2], &[CTL_KERN, KERN_HOSTNAME]);
}

#[test]
fn short_mib_output_is_enomem() {
    let mut out = [0i32; 1];
    assert_eq!(
        sysctlnametomib(b"kern.hostname", &mut out),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn read_text_value() {
    let platform = platform();
    let mut buffer = [0u8; 32];
    let length = sysctlbyname(&platform, b"kern.hostname", Some(&mut buffer), None).expect("read");
    assert_eq!(&buffer[..length], b"raven");
}

#[test]
fn read_integer_32() {
    let platform = platform();
    let mut buffer = [0u8; 4];
    let length = sysctlbyname(&platform, b"hw.ncpu", Some(&mut buffer), None).expect("read");
    assert_eq!(length, 4);
    assert_eq!(i32::from_ne_bytes(buffer), 4);
}

#[test]
fn read_integer_64() {
    let platform = platform();
    let mut buffer = [0u8; 8];
    sysctlbyname(&platform, b"hw.pagesize", Some(&mut buffer), None).expect("read");
    assert_eq!(i64::from_ne_bytes(buffer), 4096);
}

#[test]
fn short_value_buffer_is_enomem() {
    let platform = platform();
    let mut buffer = [0u8; 2];
    assert_eq!(
        sysctlbyname(&platform, b"kern.hostname", Some(&mut buffer), None),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn empty_buffer_is_a_size_query() {
    let platform = platform();
    let mut buffer = [0u8; 0];
    let length = sysctlbyname(&platform, b"kern.hostname", Some(&mut buffer), None).expect("size");
    assert_eq!(length, 6);
}

#[test]
fn write_integer_value() {
    let platform = platform();
    let value = 8i32.to_ne_bytes();
    sysctlbyname(&platform, b"hw.ncpu", None, Some(&value)).expect("write");
    let mut buffer = [0u8; 4];
    sysctlbyname(&platform, b"hw.ncpu", Some(&mut buffer), None).expect("read");
    assert_eq!(i32::from_ne_bytes(buffer), 8);
}

#[test]
fn write_text_value() {
    let platform = platform();
    sysctlbyname(&platform, b"kern.hostname", None, Some(b"other")).expect("write");
    let mut buffer = [0u8; 32];
    let length = sysctlbyname(&platform, b"kern.hostname", Some(&mut buffer), None).expect("read");
    assert_eq!(&buffer[..length], b"other");
}

#[test]
fn numeric_sysctl_matches_the_name_form() {
    let platform = platform();
    let mut buffer = [0u8; 4];
    let length = sysctl(&platform, &[CTL_HW, HW_NCPU], Some(&mut buffer), None).expect("sysctl");
    assert_eq!(length, 4);
    assert_eq!(i32::from_ne_bytes(buffer), 4);
}

#[test]
fn unknown_mib_is_enoent() {
    let platform = platform();
    assert_eq!(sysctl(&platform, &[1, 999], None, None), Err(Errno::ENOENT));
}
