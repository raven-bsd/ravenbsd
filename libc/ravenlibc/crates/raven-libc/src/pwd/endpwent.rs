// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `endpwent(3)`.

/// Drop the passwd enumeration cursor.
#[unsafe(no_mangle)]
pub extern "C" fn endpwent() {
    crate::ffi::tcbbuf::with_area(|area| {
        crate::pwd::cursor::clear::<raven_libc_pwd::Passwd>(&mut area.passwd.cursor)
    });
}
