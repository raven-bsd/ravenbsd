// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use raven_libc_stdio::Fail;

/// `putwchar_unlocked`.
#[unsafe(no_mangle)]
pub extern "C" fn putwchar_unlocked(wc: u32) -> u32 {
    match raven_libc_stdio::putwchar(crate::stdio::streams::streams().state(), wc) {
        Ok(value) => value,
        Err(Fail::Errno(error)) => {
            crate::ffi::errno::set(error);
            u32::MAX
        }
        Err(Fail::Eof) => u32::MAX,
    }
}
