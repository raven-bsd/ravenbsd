// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn null_arguments_are_rejected_before_any_query() {
    // SAFETY: the process h_errno cell.
    unsafe { crate::netdb::h_errno::h_errno = 0 };
    // SAFETY: the wrapper checks both pointers before using them.
    let result = unsafe { res_query(core::ptr::null(), 1, 1, core::ptr::null_mut(), 0) };
    assert_eq!(result, -1);
    assert_eq!(
        crate::ffi::errno::get(),
        i32::from(raven_libc_abi::errno::Errno::EINVAL.0)
    );
    // SAFETY: the process h_errno cell.
    assert_eq!(unsafe { crate::netdb::h_errno::h_errno }, NO_RECOVERY);
}
