// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the backend seam types.

use super::*;
use crate::local_backend::LocalBackend;

#[test]
fn wait_result_has_the_documented_shape() {
    let result = WaitResult {
        pid: 7,
        status: 0x100,
        siginfo: [0; 128],
        rusage: [0; 144],
    };
    assert_eq!(result.pid, 7);
    assert_eq!(result.status, 0x100);
    assert_eq!(result.siginfo.len(), 128);
    assert_eq!(result.rusage.len(), 144);
}

#[test]
fn fd_map_entry_is_two_words() {
    let entry = FdMapEntry { fd: 3, handle: 8 };
    assert_eq!(entry.fd, 3);
    assert_eq!(entry.handle, 8);
    assert_eq!(core::mem::size_of::<FdMapEntry>(), 8);
}

#[test]
fn the_backend_trait_is_object_safe() {
    let backend = LocalBackend::new(None);
    let object: &dyn ProcBackend = &backend;
    assert_eq!(object.getpid(), 1);
}

#[test]
fn early_spawn_is_object_safe() {
    struct Spawner;
    impl EarlySpawn for Spawner {
        fn spawn_initfs(
            &self,
            _path: &[u8],
            _argv: &[Vec<u8>],
            _envp: &[Vec<u8>],
            _fds: &[FdMapEntry],
        ) -> Result<i32, Errno> {
            Ok(7)
        }
    }
    let spawner = Spawner;
    let object: &dyn EarlySpawn = &spawner;
    assert_eq!(object.spawn_initfs(b"x", &[], &[], &[]), Ok(7));
}
