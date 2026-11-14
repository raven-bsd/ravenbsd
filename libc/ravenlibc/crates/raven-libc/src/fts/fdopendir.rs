// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fdopendir(3)`.
use alloc::boxed::Box;
use core::ffi::c_int;

use raven_libc_fts::{DirStream, Fd};

use super::ShellDir;
use crate::platform::ShellDirSys;

/// Wrap an owned directory descriptor.
///
/// # Safety
/// `fd` is an open descriptor whose ownership passes to the stream on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fdopendir(fd: c_int) -> *mut ShellDir {
    let mut sys = ShellDirSys;
    match DirStream::fdopendir(&mut sys, Fd(fd)) {
        Ok(stream) => Box::into_raw(ShellDir::new(stream)),
        Err(error) => crate::ffi::ret::null(error),
    }
}
