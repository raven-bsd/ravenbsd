// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uname` tests.

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::sysinfo::uname::{field_bytes, raw_uname, uname};

#[test]
fn uname_fills_every_field_from_the_syscall() {
    let platform = Platform::new(FakeSys::new());
    let record = uname(&platform).expect("uname");
    assert_eq!(record.sysname[0], b'R' as core::ffi::c_char);
    assert_eq!(record.nodename[0], b'f' as core::ffi::c_char);
    assert_eq!(record.release[0], b'0' as core::ffi::c_char);
    assert_eq!(record.version[0], b'h' as core::ffi::c_char);
    assert_eq!(record.machine[0], b'x' as core::ffi::c_char);
}

#[test]
fn libc_record_has_the_domainname_field() {
    let platform = Platform::new(FakeSys::new());
    let record = uname(&platform).expect("uname");
    assert_eq!(record.domainname, [0; 65]);
}

#[test]
fn raw_uname_exposes_the_model() {
    let platform = Platform::new(FakeSys::new());
    let record = raw_uname(&platform).expect("uname");
    assert_eq!(
        raven_abi::uname::Uname::field_str(&record.sysname),
        Some("RavenBSD")
    );
}

#[test]
fn field_bytes_stops_at_nul() {
    let mut field = [0x41u8; 65];
    field[3] = 0;
    assert_eq!(field_bytes(&field), b"AAA");
}

#[test]
fn field_bytes_without_nul_is_the_whole_field() {
    let field = [0x41u8; 65];
    assert_eq!(field_bytes(&field).len(), 65);
}
