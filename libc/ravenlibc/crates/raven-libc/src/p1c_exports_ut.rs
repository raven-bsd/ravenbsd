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
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::USER_END;
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::USER_END;
use raven_libc_abi::errno::{EBADF, EFAULT, EINVAL, ENOSYS};
use raven_libc_abi::mman::MADV_NORMAL;
use raven_libc_abi::structs::{Pollfd, SigsetT, Timespec};

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

/// The donor `t_pollts.c:err` / `t_poll.c:err` pointer, `(struct pollfd *)-1`.
///
/// The call must return `-1`/`EFAULT` without ever dereferencing the pointer. It runs in a forked
/// child so that a missing check is a fatal signal in the child (the page fault on the current
/// path) and never kills the test process; the parent asserts the exact return and errno through
/// the child's exit code.
#[test]
fn ppoll_rejects_the_donor_invalid_record_pointer_with_efault() {
    // SAFETY: plain host libc declarations with the C signatures.
    unsafe extern "C" {
        fn fork() -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn _exit(status: i32) -> !;
    }

    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: fork takes no pointers. nextest runs one test per process, so the child has no
    // other threads and only calls `ppoll` and `_exit`.
    let pid = unsafe { fork() };
    assert!(pid >= 0, "fork failed: {pid}");
    if pid == 0 {
        // SAFETY: `(struct pollfd *)-1` is the donor invalid-pointer case; the wrapper must
        // reject it before forming a slice, so this call must not touch the address.
        let result = unsafe {
            ppoll(
                usize::MAX as *mut Pollfd,
                1,
                core::ptr::null(),
                core::ptr::null(),
            )
        };
        let code = if result == -1 && errno::get() == EFAULT {
            0
        } else if result == -1 {
            1
        } else {
            2
        };
        // SAFETY: ends the forked child without running the parent's test harness.
        unsafe { _exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is the child forked above and `status` is a live out pointer.
    let reaped = unsafe { waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid did not reap the ppoll child");
    let signal = status & 0x7f;
    assert_eq!(
        signal, 0,
        "ppoll((struct pollfd *)-1) died from signal {signal}; it must return -1/EFAULT"
    );
    let code = (status >> 8) & 0xff;
    assert_eq!(
        code, 0,
        "ppoll((struct pollfd *)-1): 0 = -1/EFAULT, 1 = wrong errno, 2 = non-negative return"
    );
}

/// The `timespec *` and `sigset_t *` arguments are dereferenced too, so both take the same
/// invalid-pointer contract (`kern_poll_kfds` copyin / `kern_sigprocmask` EFAULT). An empty
/// record array keeps each case separated from the record-pointer check.
#[test]
fn ppoll_rejects_invalid_timeout_and_sigmask_pointers_with_efault() {
    // SAFETY: plain host libc declarations with the C signatures.
    unsafe extern "C" {
        fn fork() -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn _exit(status: i32) -> !;
    }

    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: fork takes no pointers; the child only calls `ppoll` and `_exit`.
    let pid = unsafe { fork() };
    assert!(pid >= 0, "fork failed: {pid}");
    if pid == 0 {
        // SAFETY: each `-1` pointer is the documented EFAULT path and must be rejected before
        // the wrapper dereferences it.
        let (timeout_ok, sigmask_ok) = unsafe {
            let timeout = ppoll(
                core::ptr::null_mut(),
                0,
                usize::MAX as *const Timespec,
                core::ptr::null(),
            );
            let timeout_ok = timeout == -1 && errno::get() == EFAULT;
            errno::set_raw(0);
            let sigmask = ppoll(
                core::ptr::null_mut(),
                0,
                core::ptr::null(),
                usize::MAX as *const SigsetT,
            );
            (timeout_ok, sigmask == -1 && errno::get() == EFAULT)
        };
        let code = match (timeout_ok, sigmask_ok) {
            (true, true) => 0,
            (false, true) => 1,
            (true, false) => 2,
            (false, false) => 3,
        };
        // SAFETY: ends the forked child without running the parent's test harness.
        unsafe { _exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is the child forked above and `status` is a live out pointer.
    let reaped = unsafe { waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid did not reap the ppoll child");
    let signal = status & 0x7f;
    assert_eq!(
        signal, 0,
        "ppoll invalid timespec/sigmask pointer died from signal {signal}"
    );
    let code = (status >> 8) & 0xff;
    assert_eq!(
        code, 0,
        "ppoll pointer check: 0 = both -1/EFAULT, 1 = timespec wrong, 2 = sigmask wrong, 3 = both wrong"
    );
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

// --- S173b separating tests (experiment; see tickets/evidence/artifacts/S173b) ---------------

/// Control: a mapped record array inside the user window is accepted.
#[test]
fn s173b_ppoll_accepts_a_mapped_in_window_record_pointer() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    let mut records = [Pollfd {
        fd: -1,
        events: 0,
        revents: 0,
    }];
    let address = records.as_mut_ptr() as usize;
    assert!(
        raven_libc_platform::poll::validate_user_range(
            address as u64,
            core::mem::size_of::<Pollfd>() as u64
        )
        .is_ok(),
        "the stack record array is inside the user window"
    );
    // SAFETY: `records` is a live writable in-window array.
    let result = unsafe {
        ppoll(
            records.as_mut_ptr(),
            1,
            core::ptr::null(),
            core::ptr::null(),
        )
    };
    assert_eq!(result, 0);
    assert_eq!(errno::get(), 0);
}

/// The S173a gap: `0x1000` is inside `USER_MIN..=USER_END` but unmapped. The wrapper must copy
/// it in (or otherwise fail safely) and return `-1`/`EFAULT`, not fault.
#[test]
fn s173b_ppoll_rejects_an_unmapped_in_window_record_pointer_with_efault() {
    // SAFETY: plain host libc declarations with the C signatures.
    unsafe extern "C" {
        fn fork() -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn _exit(status: i32) -> !;
    }

    let _guard = TestLock::acquire();
    errno::set_raw(0);
    // SAFETY: fork takes no pointers; the child only calls `ppoll` and `_exit`.
    let pid = unsafe { fork() };
    assert!(pid >= 0, "fork failed: {pid}");
    if pid == 0 {
        let pointer = 0x1000usize as *mut Pollfd;
        assert!(
            raven_libc_platform::poll::validate_user_range(
                pointer as u64,
                core::mem::size_of::<Pollfd>() as u64
            )
            .is_ok(),
            "0x1000 is inside the user window; the range check alone cannot reject it"
        );
        // SAFETY: `0x1000` is the documented unmapped in-window address; the call must reject it
        // before it reads the record.
        let result = unsafe { ppoll(pointer, 1, core::ptr::null(), core::ptr::null()) };
        let code = if result == -1 && errno::get() == EFAULT {
            0
        } else if result == -1 {
            1
        } else {
            2
        };
        // SAFETY: ends the forked child without running the parent's test harness.
        unsafe { _exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is the child forked above and `status` is a live out pointer.
    let reaped = unsafe { waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid did not reap the ppoll child");
    let signal = status & 0x7f;
    assert_eq!(
        signal, 0,
        "ppoll(0x1000, 1) died from signal {signal}; an unmapped in-window pointer must return -1/EFAULT"
    );
    let code = (status >> 8) & 0xff;
    assert_eq!(
        code, 0,
        "ppoll(0x1000): 0 = -1/EFAULT, 1 = wrong errno, 2 = non-negative return"
    );
}

/// Test-local mutation switch for the input-copy failure test.
///
/// `S173B_MUTATION_IGNORE_COPY=1` makes the copy double report success without copying, which is
/// exactly what an ignored-error helper does to `ppoll`. The test must then fail its EFAULT
/// assertion. The switch only selects the test double; no production source is touched.
fn s173b_mutation_ignores_input_copy() -> bool {
    matches!(
        std::env::var("S173B_MUTATION_IGNORE_COPY").as_deref(),
        Ok("1")
    )
}

/// The input `USER_COPY` failure must return `-1`/`EFAULT` before the shell forms a slice over
/// the caller's array. The double fails the copy, so the in-window but unmapped `0x1000` source
/// is only ever handed to the copy function; a read by the wrapper would fault. Forked so a
/// missing propagation is a child signal or exit code, never the test process.
///
/// Under `S173B_MUTATION_IGNORE_COPY=1` the same double returns `Ok(())` without copying, so the
/// shell walks past the failed snapshot and the call is no longer `-1`/`EFAULT`; the EFAULT
/// assertion fails. That is the compliant mutation, selected test-locally, not by editing the
/// production `ppoll`/seam.
#[test]
fn s173b_ppoll_input_copy_failure_is_efault_without_reading_the_caller_array() {
    // SAFETY: plain host libc declarations with the C signatures.
    unsafe extern "C" {
        fn fork() -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn _exit(status: i32) -> !;
    }

    let _guard = TestLock::acquire();
    let mutation = s173b_mutation_ignores_input_copy();
    // SAFETY: fork takes no pointers; the child only calls the wrapper and `_exit`.
    let pid = unsafe { fork() };
    assert!(pid >= 0, "fork failed: {pid}");
    if pid == 0 {
        errno::set_raw(0);
        let pointer = 0x1000usize as *mut Pollfd;
        assert!(
            raven_libc_platform::poll::validate_user_range(
                pointer as u64,
                core::mem::size_of::<Pollfd>() as u64
            )
            .is_ok(),
            "0x1000 passes the window check; only the copy can reject it"
        );
        let mut calls = 0u32;
        let mut request_matched = false;
        let mut snapshot_distinct = false;
        let result = ppoll_with_copy(
            pointer,
            1,
            core::ptr::null(),
            core::ptr::null(),
            |destination, source, length| {
                calls += 1;
                request_matched =
                    source == pointer.cast::<u8>() && length == core::mem::size_of::<Pollfd>();
                snapshot_distinct = !core::ptr::eq(destination.cast_const(), source);
                if mutation {
                    // The ignored-error mutation: report success without copying, leaving the
                    // zeroed snapshot in place, so the wrapper must not return EFAULT.
                    Ok(())
                } else {
                    Err(raven_libc_platform::Errno(EFAULT as u16))
                }
            },
        );
        let code = if result == -1 && errno::get() == EFAULT {
            if calls == 1 && request_matched && snapshot_distinct {
                0
            } else {
                1
            }
        } else {
            2
        };
        // SAFETY: ends the forked child without running the parent's test harness.
        unsafe { _exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is the child forked above and `status` is a live out pointer.
    let reaped = unsafe { waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid did not reap the ppoll child");
    let signal = status & 0x7f;
    assert_eq!(
        signal, 0,
        "the input-copy failure died from signal {signal}; the wrapper must not read the caller array"
    );
    let code = (status >> 8) & 0xff;
    assert_eq!(
        code, 0,
        "input copy: 0 = -1/EFAULT with one snapshot copy and no read, 1 = wrong call shape, 2 = not -1/EFAULT"
    );
}

/// A failed `revents` copy-out happens after a successful input snapshot and readiness. The
/// caller's record must stay untouched and the call must return `-1`/`EFAULT`.
#[test]
fn s173b_ppoll_revents_copy_failure_is_efault_after_readiness() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    let mut records = [Pollfd {
        fd: -1,
        events: 0,
        revents: 0,
    }];
    let caller = records.as_mut_ptr();
    let mut calls = 0u32;
    let mut copy_out_requested = false;
    let mut snapshot_source = core::ptr::null::<u8>();
    let result = ppoll_with_copy(
        caller,
        1,
        core::ptr::null(),
        core::ptr::null(),
        |destination, source, length| {
            calls += 1;
            if calls == 1 {
                return raven_libc_sys::sys::user_copy(destination, source, length);
            }
            copy_out_requested =
                destination == caller.cast::<u8>() && length == core::mem::size_of::<Pollfd>();
            snapshot_source = source;
            Err(raven_libc_platform::Errno(EFAULT as u16))
        },
    );
    assert_eq!(result, -1, "a failed revents copy-out is -1");
    assert_eq!(errno::get(), EFAULT, "a failed revents copy-out is EFAULT");
    assert_eq!(
        calls, 2,
        "the input copy and the revents copy-out both ran; the poll was reached"
    );
    assert!(
        copy_out_requested,
        "the copy-out must name the caller's record array"
    );
    assert!(
        !snapshot_source.is_null(),
        "the copy-out source is the snapshot"
    );
    assert_eq!(
        records[0].revents, 0,
        "a failed copy-out must leave the caller's record untouched"
    );
}

/// A copy double that succeeds for both operations still yields the normal poll result, so the
/// seam does not change the success path.
#[test]
fn s173b_ppoll_successful_copy_double_returns_the_normal_result() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    let mut records = [Pollfd {
        fd: -1,
        events: 0,
        revents: 0,
    }];
    let mut calls = 0u32;
    let result = ppoll_with_copy(
        records.as_mut_ptr(),
        1,
        core::ptr::null(),
        core::ptr::null(),
        |destination, source, length| {
            calls += 1;
            raven_libc_sys::sys::user_copy(destination, source, length)
        },
    );
    assert_eq!(
        result, 0,
        "an ignored descriptor with no events is not ready"
    );
    assert_eq!(errno::get(), 0, "success does not set errno");
    assert_eq!(calls, 2, "the input copy and the revents copy-out both ran");
    assert_eq!(records[0].fd, -1, "the caller's record is unchanged");
}

/// Negative control for the no-production-mutation requirement: a copy that reports success
/// without copying is what an ignored-error helper does to this call. It must not look like
/// `-1`/`EFAULT`, which is what the pair above asserts when the error is propagated.
#[test]
fn s173b_ppoll_a_swallowed_copy_error_does_not_look_like_efault() {
    let _guard = TestLock::acquire();
    errno::set_raw(0);
    let mut records = [Pollfd {
        fd: -1,
        events: 0,
        revents: 0,
    }];
    let result = ppoll_with_copy(
        records.as_mut_ptr(),
        1,
        core::ptr::null(),
        core::ptr::null(),
        |_destination, _source, _length| Ok(()),
    );
    assert!(
        !(result == -1 && errno::get() == EFAULT),
        "a copy that silently succeeds must not produce EFAULT; result={result} errno={}",
        errno::get()
    );
}

/// A mapped, in-window range is copied exactly. The `pselect` masked-wait test this increment
/// replaces remains in `tickets/evidence/artifacts/S173b/separating-tests.patch` and commit
/// `55c3e247f`; it is not part of the independent `USER_COPY` pointer fix.
#[test]
fn s173b_user_copy_snapshots_a_mapped_range() {
    let _guard = TestLock::acquire();
    let mut source = [0u8; 32];
    for (index, byte) in source.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let mut destination = [0u8; 32];
    let result =
        raven_libc_sys::sys::user_copy(destination.as_mut_ptr(), source.as_ptr(), source.len());
    assert_eq!(result, Ok(()));
    assert_eq!(destination, source);
}

/// A zero-length copy succeeds without touching either pointer, including NULL.
#[test]
fn s173b_user_copy_zero_length_needs_no_pointer() {
    // SAFETY: the contract says `len == 0` dereferences neither pointer.
    let result = raven_libc_sys::sys::user_copy(core::ptr::null_mut(), core::ptr::null(), 0);
    assert_eq!(result, Ok(()));
    // SAFETY: same contract, all-ones pointers.
    let result = raven_libc_sys::sys::user_copy(usize::MAX as *mut u8, usize::MAX as *const u8, 0);
    assert_eq!(result, Ok(()));
}

/// Overlap is `EINVAL` (this is a copy, not a `memmove`); adjacent ranges are accepted.
#[test]
fn s173b_user_copy_rejects_overlap_and_accepts_adjacent_ranges() {
    let mut bytes = [0u8; 32];
    let base = bytes.as_mut_ptr();
    // SAFETY: every range below is inside the live `bytes` array.
    unsafe {
        assert_eq!(
            raven_libc_sys::sys::user_copy(base.add(1), base, 8),
            Err(raven_libc_platform::Errno(EINVAL as u16))
        );
        assert_eq!(
            raven_libc_sys::sys::user_copy(base, base.add(1), 8),
            Err(raven_libc_platform::Errno(EINVAL as u16))
        );
        assert_eq!(
            raven_libc_sys::sys::user_copy(base, base, 8),
            Err(raven_libc_platform::Errno(EINVAL as u16))
        );
        // Adjacent both directions succeed.
        assert_eq!(raven_libc_sys::sys::user_copy(base.add(8), base, 8), Ok(()));
        assert_eq!(raven_libc_sys::sys::user_copy(base, base.add(8), 8), Ok(()));
    }
}

/// The 64 KiB bound is enforced before any byte is copied.
#[test]
fn s173b_user_copy_max_length_succeeds_and_max_plus_one_is_einval() {
    let _guard = TestLock::acquire();
    let source = vec![0x5Au8; 64 * 1024];
    let mut destination = vec![0u8; 64 * 1024];
    let result =
        raven_libc_sys::sys::user_copy(destination.as_mut_ptr(), source.as_ptr(), source.len());
    assert_eq!(result, Ok(()));
    assert_eq!(destination, source);

    let mut guarded = [0xA5u8; 8];
    let result =
        raven_libc_sys::sys::user_copy(guarded.as_mut_ptr(), source.as_ptr(), 64 * 1024 + 1);
    assert_eq!(result, Err(raven_libc_platform::Errno(EINVAL as u16)));
    assert_eq!(guarded, [0xA5u8; 8], "no byte may be written on EINVAL");
}

/// The fault cases must return `-1`/`EFAULT` without dying: an unmapped in-window page, a
/// kernel-half address, the donor `(void *)-1` and the end of the user window. Forked so a
/// missing fault-safe copy is a signal in the child, never the test process.
#[test]
fn s173b_user_copy_rejects_unmapped_kernel_half_and_out_of_window_with_efault() {
    // SAFETY: plain host libc declarations with the C signatures.
    unsafe extern "C" {
        fn fork() -> i32;
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn _exit(status: i32) -> !;
    }

    let _guard = TestLock::acquire();
    // SAFETY: fork takes no pointers; the child only calls `user_copy` and `_exit`.
    let pid = unsafe { fork() };
    assert!(pid >= 0, "fork failed: {pid}");
    if pid == 0 {
        let mut buffer = [0u8; 32];
        let pointer = buffer.as_mut_ptr();
        let source = buffer.as_ptr();
        // Each case is a safe call over a live buffer on one side and the documented bad
        // address on the other; the copy must reject the bad side without touching it.
        let all_efault = {
            let unmapped_source = raven_libc_sys::sys::user_copy(pointer, 0x1000 as *const u8, 8)
                == Err(raven_libc_platform::Errno(EFAULT as u16));
            let unmapped_destination = raven_libc_sys::sys::user_copy(0x1000 as *mut u8, source, 8)
                == Err(raven_libc_platform::Errno(EFAULT as u16));
            let kernel_source =
                raven_libc_sys::sys::user_copy(pointer, 0xFFFF_8000_0000_0000usize as *const u8, 8)
                    == Err(raven_libc_platform::Errno(EFAULT as u16));
            let all_ones = raven_libc_sys::sys::user_copy(pointer, usize::MAX as *const u8, 8)
                == Err(raven_libc_platform::Errno(EFAULT as u16));
            let window_end = raven_libc_sys::sys::user_copy(pointer, USER_END as *const u8, 8)
                == Err(raven_libc_platform::Errno(EFAULT as u16));
            unmapped_source && unmapped_destination && kernel_source && all_ones && window_end
        };
        let code = if all_efault { 0 } else { 1 };
        // SAFETY: ends the forked child without running the parent's test harness.
        unsafe { _exit(code) };
    }
    let mut status = 0;
    // SAFETY: `pid` is the child forked above and `status` is a live out pointer.
    let reaped = unsafe { waitpid(pid, &mut status, 0) };
    assert_eq!(reaped, pid, "waitpid did not reap the user_copy child");
    let signal = status & 0x7f;
    assert_eq!(
        signal, 0,
        "user_copy bad pointers died from signal {signal}; they must return EFAULT"
    );
    let code = (status >> 8) & 0xff;
    assert_eq!(
        code, 0,
        "user_copy bad pointer: 0 = every case EFAULT, 1 = a non-EFAULT result"
    );
}
