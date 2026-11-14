// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setgrent(3)`.

/// Rewind the group enumeration.
#[unsafe(no_mangle)]
pub extern "C" fn setgrent() {
    crate::ffi::tcbbuf::with_area(|area| {
        crate::pwd::cursor::reset::<raven_libc_pwd::Group>(area.group.cursor)
    });
}
