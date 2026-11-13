// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hostname tests.

use raven_abi::errno::Errno;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::sysinfo::hostname::{
    HOST_NAME_MAX, gethostname, hostname_from, sethostname, validate_hostname,
};

#[test]
fn gethostname_reads_sys_hostname() {
    let fake = FakeSys::new();
    fake.sys(b"hostname", b"raven\n");
    let platform = Platform::new(fake);
    let mut buffer = [0u8; 64];
    gethostname(&platform, &mut buffer).expect("gethostname");
    assert_eq!(&buffer[..6], b"raven\0");
}

#[test]
fn gethostname_falls_back_to_uname() {
    let platform = Platform::new(FakeSys::new());
    let mut buffer = [0u8; 64];
    gethostname(&platform, &mut buffer).expect("gethostname");
    assert_eq!(&buffer[..5], b"fake\0");
}

#[test]
fn gethostname_short_buffer_is_enametoolong() {
    let platform = Platform::new(FakeSys::new());
    let mut buffer = [0u8; 3];
    assert_eq!(
        gethostname(&platform, &mut buffer),
        Err(Errno::ENAMETOOLONG)
    );
}

#[test]
fn sethostname_writes_through_sys() {
    let fake = FakeSys::new();
    fake.sys(b"hostname", b"old\n");
    let platform = Platform::new(fake);
    sethostname(&platform, b"new").expect("sethostname");
    let value = platform.hostname_value();
    assert_eq!(value, b"new");
}

#[test]
fn hostname_64_is_accepted() {
    let name = [b'a'; 64];
    assert_eq!(validate_hostname(&name), Ok(name.as_slice()));
}

#[test]
fn hostname_65_is_einval() {
    let name = [b'a'; 65];
    assert_eq!(validate_hostname(&name), Err(Errno::EINVAL));
}

#[test]
fn hostname_with_nul_is_einval() {
    assert_eq!(validate_hostname(b"a\0b"), Err(Errno::EINVAL));
}

#[test]
fn hostname_prefers_sys_over_uname() {
    assert_eq!(hostname_from(Some(b"one\n"), b"two"), b"one");
    assert_eq!(hostname_from(Some(b"\n"), b"two"), b"two");
    assert_eq!(hostname_from(None, b"two"), b"two");
}

#[test]
fn hostname_max_is_64() {
    assert_eq!(HOST_NAME_MAX, 64);
}
