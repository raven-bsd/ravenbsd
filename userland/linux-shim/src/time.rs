// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Time queries mapped onto the two native clocks.

use raven_abi::errno::Errno;
use raven_abi::types::Timespec;
use raven_linux_abi::flags::clock;

use crate::Host;

const NATIVE_REALTIME: u32 = 0;
const NATIVE_MONOTONIC: u32 = 1;

/// Linux `clock_gettime`.
pub fn clock_gettime(host: &mut dyn Host, requested: u32) -> Result<Timespec, Errno> {
    let native = native_clock(requested)?;
    let (tv_sec, tv_nsec) = host.clock_get(native)?;
    Ok(Timespec { tv_sec, tv_nsec })
}

/// Linux `gettimeofday`: `(seconds, microseconds)` from realtime.
pub fn gettimeofday(host: &mut dyn Host) -> Result<(i64, i64), Errno> {
    let time = clock_gettime(host, clock::CLOCK_REALTIME)?;
    Ok((time.tv_sec, time.tv_nsec / 1_000))
}

/// Linux `time`: seconds since the epoch.
pub fn time(host: &mut dyn Host) -> Result<i64, Errno> {
    Ok(clock_gettime(host, clock::CLOCK_REALTIME)?.tv_sec)
}

/// Linux `clock_getres`: 1 ns normally, 1 ms for the coarse clocks.
pub fn clock_getres(requested: u32) -> Result<Timespec, Errno> {
    let resolution = match requested {
        clock::CLOCK_REALTIME
        | clock::CLOCK_MONOTONIC
        | clock::CLOCK_MONOTONIC_RAW
        | clock::CLOCK_BOOTTIME => 1,
        clock::CLOCK_MONOTONIC_COARSE | clock::CLOCK_REALTIME_COARSE => 1_000_000,
        _ => return Err(Errno::EINVAL),
    };
    Ok(Timespec {
        tv_sec: 0,
        tv_nsec: resolution,
    })
}

fn native_clock(requested: u32) -> Result<u32, Errno> {
    match requested {
        clock::CLOCK_REALTIME | clock::CLOCK_REALTIME_COARSE => Ok(NATIVE_REALTIME),
        clock::CLOCK_MONOTONIC
        | clock::CLOCK_MONOTONIC_RAW
        | clock::CLOCK_MONOTONIC_COARSE
        | clock::CLOCK_BOOTTIME => Ok(NATIVE_MONOTONIC),
        _ => Err(Errno::EINVAL),
    }
}
