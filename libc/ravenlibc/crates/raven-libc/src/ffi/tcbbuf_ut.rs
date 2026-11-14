// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_libc_pwd::{Group as EntryGroup, Passwd as EntryPasswd};

#[test]
fn area_sizes_and_alignment() {
    assert_eq!(core::mem::size_of::<InetNtoa>(), 16);
    assert_eq!(core::mem::size_of::<HostArea>(), 1024);
    assert_eq!(core::mem::size_of::<ServArea>(), 1024);
    assert_eq!(core::mem::size_of::<ProtoArea>(), 1024);
    assert_eq!(core::mem::size_of::<NetArea>(), 1024);
    assert_eq!(core::mem::size_of::<TextArea>(), 256);
    assert_eq!(core::mem::align_of::<HostArea>(), 8);
    assert_eq!(core::mem::align_of::<PwdArea>(), 8);
    assert_eq!(core::mem::align_of::<GrpArea>(), 8);
    assert_eq!(core::mem::align_of::<Area>(), 8);
    assert_eq!(core::mem::size_of::<Passwd>(), 48);
    assert_eq!(core::mem::size_of::<Group>(), 32);
}

#[test]
fn text_copy_is_bounded() {
    let mut buf = [0u8; 8];
    let pointer = copy_text(&mut buf, b"abc").expect("fits");
    // SAFETY: `copy_text` wrote a NUL-terminated string in `buf`.
    assert_eq!(unsafe { crate::ffi::cstr::slice(pointer) }, b"abc");
    let error = copy_text(&mut buf, b"12345678").expect_err("must not fit");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::ERANGE);
}

#[test]
fn passwd_record_points_into_the_buffer() {
    let entry = EntryPasswd {
        name: "root".into(),
        passwd: "x".into(),
        uid: 0,
        gid: 0,
        gecos: "root".into(),
        dir: "/root".into(),
        shell: "/bin/sh".into(),
    };
    let mut record = Passwd::ZERO;
    let mut buf = [0u8; 1024];
    fill_passwd(&entry, &mut record, &mut buf).expect("fits");
    // SAFETY: every pointer names a NUL-terminated string inside `buf`.
    unsafe {
        assert_eq!(
            crate::ffi::cstr::slice(record.pw_name.cast::<core::ffi::c_char>()),
            b"root"
        );
        assert_eq!(
            crate::ffi::cstr::slice(record.pw_dir.cast::<core::ffi::c_char>()),
            b"/root"
        );
        assert_eq!(
            crate::ffi::cstr::slice(record.pw_shell.cast::<core::ffi::c_char>()),
            b"/bin/sh"
        );
    }
    assert_eq!(record.pw_uid, 0);
}

#[test]
fn short_passwd_buffer_is_range() {
    let entry = EntryPasswd {
        name: "averylongname".into(),
        passwd: "x".into(),
        uid: 1,
        gid: 1,
        gecos: alloc::string::String::new(),
        dir: "/".into(),
        shell: "/bin/sh".into(),
    };
    let mut record = Passwd::ZERO;
    let mut buf = [0u8; 4];
    let error = fill_passwd(&entry, &mut record, &mut buf).expect_err("must not fit");
    assert_eq!(i32::from(error.0), raven_libc_abi::errno::ERANGE);
    assert!(record.pw_name.is_null());
}

#[test]
fn group_record_builds_the_member_array() {
    let entry = EntryGroup {
        name: "wheel".into(),
        passwd: "x".into(),
        gid: 0,
        mem: alloc::vec!["alice".to_string(), "bob".to_string()],
    };
    let mut record = Group::ZERO;
    let mut buf = [0u8; 1024];
    fill_group(&entry, &mut record, &mut buf).expect("fits");
    let members = record.gr_mem.cast::<*mut core::ffi::c_char>();
    assert!(!members.is_null());
    // SAFETY: `fill_group` wrote three pointer slots (two names and NULL).
    unsafe {
        let first = *members;
        let second = *members.add(1);
        let last = *members.add(2);
        assert_eq!(crate::ffi::cstr::slice(first.cast_const()), b"alice");
        assert_eq!(crate::ffi::cstr::slice(second.cast_const()), b"bob");
        assert!(last.is_null());
    }
    // SAFETY: the name pointer names a NUL-terminated string inside `buf`.
    unsafe {
        assert_eq!(
            crate::ffi::cstr::slice(record.gr_name.cast::<core::ffi::c_char>()),
            b"wheel"
        )
    };
}

#[test]
fn area_pack_is_eight_aligned_and_writable() {
    with_area(|area| {
        let pointer = area.passwd.buf.as_mut_ptr();
        assert_eq!(pointer.addr() % 8, 0);
        area.inet_ntoa.bytes[0] = b'1';
    });
}
