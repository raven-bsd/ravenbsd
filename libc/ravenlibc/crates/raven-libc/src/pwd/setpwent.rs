// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setpwent(3)`.

/// Rewind the passwd enumeration.
#[unsafe(no_mangle)]
pub extern "C" fn setpwent() {
    crate::ffi::tcbbuf::with_area(|area| {
        crate::pwd::cursor::reset::<raven_libc_pwd::Passwd>(area.passwd.cursor)
    });
}
