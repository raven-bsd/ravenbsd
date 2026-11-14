// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `endgrent(3)`.

/// Drop the group enumeration cursor.
#[unsafe(no_mangle)]
pub extern "C" fn endgrent() {
    crate::ffi::tcbbuf::with_area(|area| {
        crate::pwd::cursor::clear::<raven_libc_pwd::Group>(&mut area.group.cursor)
    });
}
