// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `FileSource` of C51's user-database parsers over C55's platform: open, read to the cap,
//! close. The source is intentionally stateless; every lookup reads the file again (C51).

use alloc::vec::Vec;

use raven_libc_abi::fcntl::O_RDONLY;
use raven_libc_nss::FileSource;

/// The shell's file seam for `/etc/passwd`, `/etc/group` and `/etc/shells`.
pub(crate) struct ShellFiles;

fn error(value: raven_libc_platform::Errno) -> i32 {
    i32::from(value.0)
}

impl FileSource for ShellFiles {
    fn read_file(&self, path: &str, max: usize) -> Result<Vec<u8>, i32> {
        let platform = crate::ffi::platform::global();
        let fd = platform
            .open(path.as_bytes(), O_RDONLY as u32, 0)
            .map_err(error)?;
        let mut out = Vec::new();
        let mut chunk = [0u8; 4096];
        while out.len() < max {
            let want = core::cmp::min(chunk.len(), max.saturating_sub(out.len()));
            let slice = match chunk.get_mut(..want) {
                Some(slice) => slice,
                None => break,
            };
            match platform.read(fd, slice) {
                Ok(0) => break,
                Ok(count) => out.extend_from_slice(match slice.get(..count) {
                    Some(bytes) => bytes,
                    None => break,
                }),
                Err(value) => {
                    let _ = platform.close(fd);
                    return Err(error(value));
                }
            }
        }
        platform.close(fd).map_err(error)?;
        Ok(out)
    }
}
