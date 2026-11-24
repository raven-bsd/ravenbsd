// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getrusage(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::{Rusage, Timeval};

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_libc_proc::syscall_backend::PROC_GETRUSAGE;

const NANOS_PER_SECOND: u64 = 1_000_000_000;
const NANOS_PER_MICROSECOND: u64 = 1_000;

/// Convert the kernel's nanosecond counter to a `struct timeval`.
fn timeval_from_ns(nanoseconds: u64) -> Timeval {
    let seconds = nanoseconds.checked_div(NANOS_PER_SECOND).unwrap_or(0);
    let remainder = nanoseconds.checked_rem(NANOS_PER_SECOND).unwrap_or(0);
    let microseconds = remainder.checked_div(NANOS_PER_MICROSECOND).unwrap_or(0);
    Timeval {
        tv_sec: i64::try_from(seconds).unwrap_or(i64::MAX),
        tv_usec: i64::try_from(microseconds).unwrap_or(i64::MAX),
    }
}

/// Map the eight `PROC_GETRUSAGE` words into the C `struct rusage`.
///
/// Words: user ns, system ns, maximum RSS bytes, minor faults, major faults, voluntary context
/// switches, involuntary context switches, start time. The BSD donor also reports user/system
/// time in `timeval` and the maximum RSS; Raven's ABI carries the RSS in bytes (the probe and the
/// kernel reference row both assert bytes).
fn rusage_from_words(words: [u64; 8]) -> Rusage {
    let [
        user_ns,
        system_ns,
        maxrss,
        minflt,
        majflt,
        nvcsw,
        nivcsw,
        _start,
    ] = words;
    Rusage {
        ru_utime: timeval_from_ns(user_ns),
        ru_stime: timeval_from_ns(system_ns),
        ru_maxrss: i64::try_from(maxrss).unwrap_or(i64::MAX),
        ru_minflt: i64::try_from(minflt).unwrap_or(i64::MAX),
        ru_majflt: i64::try_from(majflt).unwrap_or(i64::MAX),
        ru_nvcsw: i64::try_from(nvcsw).unwrap_or(i64::MAX),
        ru_nivcsw: i64::try_from(nivcsw).unwrap_or(i64::MAX),
        ..Rusage::ZERO
    }
}

/// Fill the caller's usage record for the running process.
///
/// # Safety
/// `usage` is NULL or points to a writable `struct rusage`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrusage(who: c_int, usage: *mut Rusage) -> c_int {
    if usage.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    if !matches!(who, -1..=1) {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    }
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        // `PROC_GETRUSAGE` writes eight u64 words: user ns, system ns, maxrss, minflt, majflt,
        // nvcsw, nivcsw, start. The libc used to zero the whole record; the kernel accounting was
        // already correct.
        let mut words = [0u64; 8];
        // SAFETY: `words` is a live eight-word array; the kernel validates the pointer and count
        // (eight) and writes exactly eight u64 values.
        let raw = unsafe {
            raven_libc_sys::arch::syscall3(
                PROC_GETRUSAGE,
                who as isize as usize,
                words.as_mut_ptr() as usize,
                words.len(),
            )
        };
        return match raven_libc_sys::errno::ret(raw) {
            Ok(_) => {
                // SAFETY: the caller's writable record was checked non-null above.
                unsafe { usage.write(rusage_from_words(words)) };
                0
            }
            Err(error) => {
                crate::ffi::ret::int(Err(crate::ffi::map_proc::error(i32::from(error.0))))
            }
        };
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        // SAFETY: the caller's writable `struct rusage`; the host backend has no accounting.
        unsafe { usage.write(Rusage::ZERO) };
        0
    }
}

#[cfg(test)]
#[path = "getrusage_ut.rs"]
mod tests;
