// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S21a driver-object syscalls (`MMIO_CREATE`, `BOOT_BLOB`).

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;

use crate::syscall::SyscallFrame;

/// Dispatch one driver-object syscall.
#[cfg(target_arch = "x86_64")]
pub fn dispatch(call: Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    crate::driver::dispatch(call, frame)
}
