// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `SpawnEnv` of `posix_spawn`'s file-action applier. The platform layer has no accessor
//! for the scheme handle behind one of its descriptors and no public snapshot of the inherited
//! descriptor list, so an `OPEN` action reports `ENOSYS` and a `DUP2` of an inherited
//! descriptor reports `EBADF` until that seam exists (QUESTION line in the report). Actions
//! that need no handle (`CLOSE`, `CHDIR`, `CLOSEFROM`) are applied normally.

use alloc::vec::Vec;

use raven_libc_proc::errors::{ENOSYS, Errno};
use raven_libc_proc::spawn_model::SpawnEnv;

/// The shell's file-action environment.
pub(crate) struct ShellSpawnEnv;

impl SpawnEnv for ShellSpawnEnv {
    fn open(&mut self, _path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, Errno> {
        Err(ENOSYS)
    }

    fn is_dir(&mut self, _handle: u32) -> Result<bool, Errno> {
        Err(ENOSYS)
    }

    fn close_temp(&mut self, _handle: u32) {}

    fn fd_path(&mut self, _handle: u32) -> Option<Vec<u8>> {
        None
    }
}
