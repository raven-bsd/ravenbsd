// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the P1c ABI-ready wrappers.
//!
//! The module is compiled on the host under `test` with the `no_mangle` attribute disabled, so
//! these tests call the wrappers themselves rather than a copy of their argument checks. Every
//! case here either validates a pointer/scalar argument before the platform is reached or uses a
//! platform path that returns without a Raven system call (`select` with no descriptors,
//! `ppoll`/`pselect`/`brk` returning `ENOSYS`, the documented `mlock` family no-ops and
//! `sleep(0)`).

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used
)]

use super::*;
use crate::ffi::{TestLock, errno};
use core::ffi::{c_char, c_void};
use raven_libc_abi::errno::{EBADF, EFAULT, EINVAL, ENOSYS};
use raven_libc_abi::mman::MADV_NORMAL;

fn assert_efault(result: c_int) {
    assert_eq!(result, -1);
    assert_eq!(errno::get(), EFAULT);
}

fn assert_ssize_efault(result: isize) {
    assert_eq!(result, -1);
    assert_eq!(errno::get(), EFAULT);
}

fn assert_long_efault(result: core::ffi::c_long) {
    assert_eq!(result, -1);
    assert_eq!(errno::get(), EFAULT);
}

#[test]
fn getpagesize_reports_the_target_page_size() {
    // SAFETY: no arguments.
    assert_eq!(unsafe { getpagesize() }, 4096);
}

#[test]
fn posix_close_rejects_nonzero_flags() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: scalar arguments only; the flags check returns before the platform call.
    assert_eq!(unsafe { posix_close(0, 1) }, -1);
    assert_eq!(errno::get(), EINVAL);
}

#[test]
fn null_output_pointers_are_efault() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: each NULL pointer is the documented failure path.
    unsafe {
        assert_efault(getrlimit64(0, core::ptr::null_mut()));
        assert_efault(fstatvfs(0, core::ptr::null_mut()));
        assert_efault(pipe(core::ptr::null_mut()));
        assert_ssize_efault(readlinkat(0, core::ptr::null(), core::ptr::null_mut(), 0));
    }
}

#[test]
fn null_path_pointers_are_efault() {
    let _guard = TestLock::acquire();
    let null = core::ptr::null::<c_char>();
    errno::set_raw(0);
    // SAFETY: each NULL path is the documented failure path.
    unsafe {
        assert_efault(lchmod(null, 0));
        assert_efault(mkdirat(0, null, 0));
        assert_efault(eaccess(null, 0));
        assert_efault(euidaccess(null, 0));
        assert_efault(faccessat(0, null, 0, 0));
        assert_long_efault(pathconf(null, 0));
        assert_efault(truncate64(null, 0));
        assert_efault(truncate(null, 0));
    }
}

#[test]
fn select_with_no_descriptors_returns_zero() {
    assert_eq!(
        // SAFETY: all sets and the timeout are NULL, so there is nothing to read or write.
        unsafe {
            select(
                0,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null(),
            )
        },
        0
    );
}

#[test]
fn ppoll_empty_request_returns_zero() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: NULL records/timeout/mask are the empty request.
    let result = unsafe {
        ppoll(
            core::ptr::null_mut(),
            0,
            core::ptr::null(),
            core::ptr::null(),
        )
    };
    assert_eq!(result, 0);
    assert_eq!(errno::get(), 0);
}

#[test]
fn ppoll_null_fds_with_a_nonzero_count_is_efault() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: a NULL record array with a nonzero count is the documented EFAULT path; the wrapper
    // must reject it before forming a slice.
    let result = unsafe {
        ppoll(
            core::ptr::null_mut(),
            1,
            core::ptr::null(),
            core::ptr::null(),
        )
    };
    assert_eq!(result, -1);
    assert_eq!(errno::get(), EFAULT);
}

#[test]
fn pselect_empty_request_is_enosys_until_the_signal_swap_lands() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: NULL sets/timeout/mask are the empty request.
    let result = unsafe {
        pselect(
            0,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null(),
            core::ptr::null(),
        )
    };
    assert_eq!(result, -1);
    assert_eq!(errno::get(), ENOSYS);
}

#[test]
fn the_documented_lock_family_is_a_no_op() {
    // SAFETY: the address/length pair is page aligned and never dereferenced.
    unsafe {
        assert_eq!(mlockall(0), 0);
        assert_eq!(munlockall(), 0);
        assert_eq!(mlock(0x1000 as *const c_void, 4096), 0);
        assert_eq!(munlock(0x1000 as *const c_void, 4096), 0);
        assert_eq!(mlock2(0x1000 as *const c_void, 4096, 0), 0);
    }
}

#[test]
fn documented_memory_advice_succeeds_and_bad_arguments_fail() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    assert_eq!(
        // SAFETY: the address is page aligned; MADV_NORMAL is a documented no-op.
        unsafe { madvise(0x1000 as *mut c_void, 4096, MADV_NORMAL) },
        0
    );
    assert_eq!(
        // SAFETY: same contract, POSIX spelling.
        unsafe { posix_madvise(0x1000 as *mut c_void, 4096, MADV_NORMAL) },
        0
    );
    assert_eq!(
        // SAFETY: the unaligned address is rejected before any dereference.
        unsafe { madvise(0x1001 as *mut c_void, 4096, MADV_NORMAL) },
        -1
    );
    assert_eq!(errno::get(), EINVAL);
}

#[test]
fn brk_is_class_b_enosys() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: scalar address only; the platform returns ENOSYS.
    assert_eq!(unsafe { brk(core::ptr::null_mut()) }, -1);
    assert_eq!(errno::get(), ENOSYS);
}

#[test]
fn sleep_zero_returns_immediately() {
    // SAFETY: no pointers; `sleep(0)` is the documented no-op path.
    assert_eq!(unsafe { sleep(0) }, 0);
}

#[test]
fn fd_set_conversion_handles_null_and_a_real_set() {
    // SAFETY: NULL is the documented empty set.
    let empty = unsafe { fd_set(core::ptr::null_mut()) };
    assert!(empty.is_empty());

    let mut set = FdSet::ZERO;
    // SAFETY: `set` is a live writable fd_set.
    let words = unsafe { fd_set(&mut set) };
    assert_eq!(words.len(), 16);
    words[0] = 0b101;
    assert_eq!(set.bytes[0], 0b101);
}

#[test]
fn iovec_conversion_copies_the_caller_view() {
    let mut first = [b'a'; 3];
    let mut second = [b'b'; 2];
    let mut records = [
        Iovec {
            iov_base: first.as_mut_ptr().cast::<c_void>(),
            iov_len: first.len() as u64,
        },
        Iovec {
            iov_base: second.as_mut_ptr().cast::<c_void>(),
            iov_len: second.len() as u64,
        },
    ];
    // SAFETY: `records` names two live writable buffers.
    let mut slices = unsafe { iovec_mut(records.as_mut_ptr(), 2) };
    assert_eq!(slices.len(), 2);
    slices[0][0] = b'x';
    assert_eq!(first[0], b'x');

    // SAFETY: `records` names two readable buffers.
    let slices = unsafe { iovec_ref(records.as_ptr(), 2) };
    assert_eq!(slices.len(), 2);
    assert_eq!(slices[1], &second[..]);
}

#[test]
fn iovec_conversion_rejects_negative_and_zero_counts() {
    // SAFETY: a zero count returns an empty vector without reading the pointer.
    assert!(unsafe { iovec_mut(core::ptr::null_mut(), 0) }.is_empty());
    // SAFETY: a negative count is treated as zero.
    assert!(unsafe { iovec_ref(core::ptr::null(), -1) }.is_empty());
}

#[test]
fn ttyname_returns_null_for_an_unknown_descriptor() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: scalar descriptor only; the descriptor is absent from the empty fd table.
    assert!(unsafe { ttyname(0) }.is_null());
    assert_eq!(errno::get(), EBADF);
}
