// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ptsname` shell tests: the static buffer wrapper over `ptsname_r`.

use super::*;

#[test]
fn an_unknown_fd_returns_null() {
    // The pty pair map has no entry for -1; ptsname_r returns EBADF and ptsname returns NULL.
    // SAFETY: `ptsname(-1)` is the documented invalid-fd probe; the wrapper only reads the
    // process pty pair map and returns NULL on EBADF, so no pointer is dereferenced.
    let pointer = unsafe { ptsname(-1) };
    assert!(pointer.is_null());
}
