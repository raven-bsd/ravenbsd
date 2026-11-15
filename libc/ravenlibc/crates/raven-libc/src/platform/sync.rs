// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sync(2)`.
/// Flush all filesystems.
#[unsafe(no_mangle)]
pub extern "C" fn sync() {
    let _ = crate::ffi::platform::global().sync();
}
