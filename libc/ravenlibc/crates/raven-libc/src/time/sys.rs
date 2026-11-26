// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `TimeSys` over the platform layer.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Timespec;
use raven_libc_time::{ClockId, TimeSys};

use crate::ffi::platform;

/// The process time seam.
#[derive(Clone, Copy, Default)]
pub(crate) struct ShellTime;

/// The monotonic clock through the public-to-wire translation above.
pub(crate) fn monotonic_now() -> Option<Timespec> {
    ShellTime.clock_get(ClockId::Monotonic).ok()
}

fn error(value: raven_libc_platform::Errno) -> Errno {
    Errno(value.0)
}

impl TimeSys for ShellTime {
    fn clock_get(&self, id: ClockId) -> Result<Timespec, Errno> {
        // The public FreeBSD clock IDs are 0 (realtime) and 4 (monotonic); the kernel wire
        // encoding stays 0 and 1. Translate at this seam, not in the public ABI.
        #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
        {
            let wire = match id {
                ClockId::Realtime => 0u32,
                ClockId::Monotonic => 1u32,
            };
            let time = raven_libc_sys::sys::clock_get(wire).map_err(|error| Errno(error.0))?;
            return Ok(Timespec {
                tv_sec: time.tv_sec,
                tv_nsec: time.tv_nsec,
            });
        }
        #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
        {
            let clock = match id {
                ClockId::Realtime => raven_libc_abi::time::CLOCK_REALTIME,
                ClockId::Monotonic => raven_libc_abi::time::CLOCK_MONOTONIC,
            };
            raven_libc_platform::time::clock_gettime(platform::global(), clock).map_err(error)
        }
    }

    fn nanosleep(&self, ns: u64) -> Result<(), Errno> {
        raven_libc_platform::time::nanosleep(platform::global(), ns)
            .map(|_| ())
            .map_err(error)
    }

    fn read_file(&self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        let fd = platform::global()
            .open(path, raven_libc_abi::fcntl::O_RDONLY as u32, 0)
            .map_err(error)?;
        let result = platform::global().read(fd, buf).map_err(error);
        let _ = platform::global().close(fd);
        result
    }

    fn issetugid(&self) -> bool {
        false
    }

    fn process_times(&self) -> Result<(u64, u64), Errno> {
        let mut values = [0u64; 4];
        // SAFETY: PROC_TIMES writes the four u64 words into the caller's 32-byte buffer.
        let result = unsafe {
            raven_libc_sys::arch::syscall2(0x4C, values.as_mut_ptr() as usize, values.len())
        };
        if result < 0 {
            return Err(Errno(u16::try_from(-result).unwrap_or(5)));
        }
        Ok((values[0], values[1]))
    }
}
