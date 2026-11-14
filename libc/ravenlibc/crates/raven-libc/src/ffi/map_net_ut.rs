// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use raven_libc_resolv::Eai;

#[test]
fn eai_codes_are_the_musl_values() {
    assert_eq!(eai_code(Eai::NoName), -2);
    assert_eq!(eai_code(Eai::Again), -3);
    assert_eq!(eai_code(Eai::Memory), -10);
}

#[test]
fn eai_texts_and_unknown() {
    assert_eq!(gai_text(-2), "Name does not resolve");
    assert_eq!(gai_text(-10), "Out of memory");
    assert_eq!(gai_text(7), "Unknown error");
}

#[test]
fn legacy_error_texts() {
    assert_eq!(hstrerror_text(1), "Host not found");
    assert_eq!(hstrerror_text(4), "Address not available");
    assert_eq!(hstrerror_text(0), "Unknown error");
}

#[test]
fn herror_line_has_the_prefix_and_newline() {
    assert_eq!(herror_line(b"prog", 1), b"prog: Host not found\n");
    assert_eq!(herror_line(b"", 2), b": Try again\n");
}

#[test]
fn h_errno_mapping() {
    assert_eq!(h_errno_for(Eai::NoName), 1);
    assert_eq!(h_errno_for(Eai::Again), 2);
    assert_eq!(h_errno_for(Eai::System), 3);
}

#[test]
fn pton_and_ntop_round_trip() {
    let _guard = crate::ffi::TestLock::acquire();
    let mut address = [0u8; 4];
    assert_eq!(
        pton(raven_libc_abi::socket::AF_INET, b"127.0.0.1", &mut address),
        1
    );
    assert_eq!(address, [127, 0, 0, 1]);
    let mut text = [0u8; 16];
    let pointer = ntop(raven_libc_abi::socket::AF_INET, &address, &mut text);
    assert!(!pointer.is_null());
    // SAFETY: `ntop` wrote a NUL-terminated string.
    assert_eq!(unsafe { crate::ffi::cstr::slice(pointer) }, b"127.0.0.1");
}

#[test]
fn pton_rejects_a_bad_family_and_ntop_a_short_buffer() {
    let _guard = crate::ffi::TestLock::acquire();
    assert_eq!(pton(99, b"1.2.3.4", &mut []), -1);
    assert_eq!(
        crate::ffi::errno::get(),
        raven_libc_abi::errno::EAFNOSUPPORT
    );
    let address = [127u8, 0, 0, 1];
    let mut text = [0u8; 4];
    assert!(ntop(raven_libc_abi::socket::AF_INET, &address, &mut text).is_null());
    assert_eq!(crate::ffi::errno::get(), raven_libc_abi::errno::ENOSPC);
}
