// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `rewinddir(3)`.
use super::ShellDir;
use crate::platform::ShellDirSys;

/// Reposition a directory stream at its start.
///
/// # Safety
/// `dir` is a live `DIR`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rewinddir(dir: *mut ShellDir) {
    if dir.is_null() {
        return;
    }
    // SAFETY: the caller's DIR.
    let state = unsafe { &mut *dir };
    let mut sys = ShellDirSys;
    state.stream.rewinddir(&mut sys);
}
