// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the endmntent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `endmntent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

/// `endmntent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn endmntent(file: *mut super::File) -> c_int {
    if file.is_null() {
        return 1;
    }
    // SAFETY: `file` is the stream from `setmntent`.
    unsafe { super::fclose(file) };
    1
}
