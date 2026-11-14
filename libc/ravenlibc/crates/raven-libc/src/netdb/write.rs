// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `herror`/`psignal`-style text writer over the platform's `write`, never stdio.

use raven_libc_abi::errno::Errno;

/// Write every byte to `fd`, retrying a short write.
pub(crate) fn write_all(fd: i32, bytes: &[u8]) -> Result<(), Errno> {
    let platform = crate::ffi::platform::global();
    let mut written = 0usize;
    while written < bytes.len() {
        let Some(rest) = bytes.get(written..) else {
            break;
        };
        match platform.write(fd, rest) {
            Ok(0) => return Err(Errno(raven_libc_abi::errno::EIO as u16)),
            Ok(count) => written = written.saturating_add(count),
            Err(error) => return Err(Errno(error.0)),
        }
    }
    Ok(())
}
