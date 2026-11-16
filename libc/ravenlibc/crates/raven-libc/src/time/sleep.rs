// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sleep` and `usleep` over the C56 sleep model.

use core::ffi::{c_int, c_uint};

use raven_libc_abi::structs::Timespec;
use raven_libc_time::sleep::{SleepErr, nanosleep};

use super::sys::ShellTime;

/// `sleep`: seconds, returning the unslept remainder on interruption.
#[unsafe(no_mangle)]
pub extern "C" fn sleep(seconds: c_uint) -> c_uint {
    let request = Timespec {
        tv_sec: i64::from(seconds),
        tv_nsec: 0,
    };
    match nanosleep(&ShellTime, &request) {
        Ok(()) => 0,
        Err(SleepErr { rem, .. }) => u32::try_from(rem.tv_sec.max(0)).unwrap_or(0),
    }
}

/// `usleep`: microseconds, returning `-1` and setting `errno` on failure.
#[unsafe(no_mangle)]
pub extern "C" fn usleep(microseconds: c_uint) -> c_int {
    let nanoseconds = u64::from(microseconds).saturating_mul(1_000);
    let request = Timespec {
        tv_sec: i64::try_from(nanoseconds / 1_000_000_000).unwrap_or(i64::MAX),
        tv_nsec: i64::try_from(nanoseconds % 1_000_000_000).unwrap_or(0),
    };
    match nanosleep(&ShellTime, &request) {
        Ok(()) => 0,
        Err(SleepErr { err, .. }) => {
            crate::ffi::errno::set(err);
            -1
        }
    }
}
