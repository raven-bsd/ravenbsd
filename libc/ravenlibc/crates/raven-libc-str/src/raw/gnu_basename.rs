// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// CLEANROOM: the behaviour is the ticket's GNU basename contract and its vectors.
// The bionic BSD-2 implementation was read only to confirm the trailing-slash rule;
// bionic is not a donors.lock entry, so the file claims no import and reproduces no
// donor text. The implementation below is an independent iterator formulation.

//! `__gnu_basename` pointer entry point.

/// Pointer to the last path component (the byte after the last `/`), or `path` itself.
///
/// # Safety
///
/// `path` is a NUL-terminated string.
pub unsafe fn gnu_basename(path: *const u8) -> *mut u8 {
    // SAFETY: `strlen` stays inside the string; the offset is at most the length, so the result
    // is inside the object (including the NUL).
    unsafe {
        let length = super::strlen::strlen(path);
        let offset = crate::gnu_basename::offset(super::common::bytes(path, length));
        path.add(offset) as *mut u8
    }
}
