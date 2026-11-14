// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `user_from_uid(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_pwd::{FilesSource, PwCache};

/// The login name of `uid`, or the decimal uid when `noname` is non-zero.
#[unsafe(no_mangle)]
pub extern "C" fn user_from_uid(uid: u32, noname: c_int) -> *mut c_char {
    let source = FilesSource::new(&crate::pwd::source::ShellFiles);
    let mut cache = PwCache::new(&source, &source);
    const MISSING: &str = "\u{0}";
    match cache.user_from_uid(uid, MISSING) {
        Ok(name) if name == MISSING => {
            if noname == 0 {
                return core::ptr::null_mut();
            }
            crate::ffi::tcbbuf::with_area(|area| {
                crate::ffi::tcbbuf::write_u32(&mut area.name.bytes, uid)
                    .unwrap_or(core::ptr::null_mut())
            })
        }
        Ok(name) => crate::ffi::tcbbuf::with_area(|area| {
            crate::ffi::tcbbuf::copy_text(&mut area.name.bytes, name.as_bytes())
                .unwrap_or(core::ptr::null_mut())
        }),
        Err(error) => crate::ffi::ret::null(raven_libc_abi::errno::Errno(error as u16)),
    }
}
