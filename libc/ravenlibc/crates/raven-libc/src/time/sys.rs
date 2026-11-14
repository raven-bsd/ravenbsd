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

fn error(value: raven_libc_platform::Errno) -> Errno {
    Errno(value.0)
}

impl TimeSys for ShellTime {
    fn clock_get(&self, id: ClockId) -> Result<Timespec, Errno> {
        let clock = match id {
            ClockId::Realtime => raven_libc_abi::time::CLOCK_REALTIME,
            ClockId::Monotonic => raven_libc_abi::time::CLOCK_MONOTONIC,
        };
        raven_libc_platform::time::clock_gettime(platform::global(), clock).map_err(error)
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
}
