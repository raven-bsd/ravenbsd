// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_short_name_round_trips() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(setname(&kernel, kernel.tcb(), b"worker"), Ok(()));
    let mut buffer = [0u8; 16];
    assert_eq!(getname(&kernel, kernel.tcb(), &mut buffer), 6);
    assert_eq!(&buffer[..6], b"worker");
    assert_eq!(buffer[6], 0);
}

#[test]
fn a_fifteen_byte_name_fits() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(setname(&kernel, kernel.tcb(), b"123456789012345"), Ok(()));
    assert_eq!(read_name(kernel.tcb())[..15], *b"123456789012345");
    assert_eq!(read_name(kernel.tcb())[15], 0);
}

#[test]
fn a_longer_name_is_erange_class() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(
        setname(&kernel, kernel.tcb(), b"1234567890123456"),
        Err(crate::limits::ENAMETOOLONG)
    );
}

#[test]
fn setting_a_name_clears_the_rest() {
    let kernel = crate::test_support::MockKernel::new();
    setname(&kernel, kernel.tcb(), b"abcdef").unwrap();
    setname(&kernel, kernel.tcb(), b"x").unwrap();
    let name = read_name(kernel.tcb());
    assert_eq!(name[0], b'x');
    assert_eq!(name[1], 0);
}

#[test]
fn a_null_thread_is_esrch() {
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(setname(&kernel, 0, b"x"), Err(crate::limits::ESRCH));
}

#[test]
fn getname_respects_a_short_buffer() {
    let kernel = crate::test_support::MockKernel::new();
    setname(&kernel, kernel.tcb(), b"worker").unwrap();
    let mut buffer = [0xffu8; 4];
    let length = getname(&kernel, kernel.tcb(), &mut buffer);
    assert_eq!(length, 3);
    assert_eq!(&buffer[..3], b"wor");
    assert_eq!(buffer[3], 0);
}

#[test]
fn an_empty_name_reads_back_empty() {
    let kernel = crate::test_support::MockKernel::new();
    let mut buffer = [0xffu8; 4];
    assert_eq!(getname(&kernel, kernel.tcb(), &mut buffer), 0);
    assert_eq!(buffer[0], 0);
}
