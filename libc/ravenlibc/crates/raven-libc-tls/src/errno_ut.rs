// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::tcb::{Pthread, pthread_to_tp};

#[test]
fn x86_constant_is_the_abi_value() {
    assert_eq!(ERRNO_OFFSET_X86_64, 52);
    #[cfg(target_arch = "x86_64")]
    assert_eq!(offset_from_tp(), ERRNO_OFFSET_X86_64);
}

#[test]
fn errno_location_is_the_tcb_cell() {
    let mut tcb = Pthread::new(0);
    let address = &mut tcb as *mut Pthread as usize;
    let location = errno_location_for(pthread_to_tp(address));
    assert_eq!(
        location as usize,
        address + core::mem::offset_of!(Pthread, errno_val)
    );
    assert_eq!(tcb.errno_val, 0);
}

#[test]
fn h_errno_location_is_inside_the_tcb() {
    let tcb = Pthread::new(0);
    let address = &tcb as *const Pthread as usize;
    let location = h_errno_location_for(pthread_to_tp(address)) as usize;
    let start = address + core::mem::offset_of!(Pthread, h_errno_val);
    assert_eq!(location, start);
    assert_eq!(
        location,
        address + core::mem::offset_of!(Pthread, h_errno_val)
    );
}

#[test]
fn tp_adjust_matches_the_struct_size() {
    #[cfg(target_arch = "x86_64")]
    assert_eq!(tp_adjust(), 0);
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    assert_eq!(tp_adjust(), core::mem::size_of::<Pthread>() as isize);
}

#[test]
fn h_errno_offset_is_in_range() {
    let offset = h_errno_offset_from_tp();
    assert!(offset.unsigned_abs() < core::mem::size_of::<Pthread>());
}
