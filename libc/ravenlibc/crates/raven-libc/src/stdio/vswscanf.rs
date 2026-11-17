// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_int, c_void};

use raven_libc_fmt::{ScanSource, ScanOptions, vscan};

use crate::stdio::va::{List, reader};

struct WideSource<'a> {
    data: &'a [u32],
    position: usize,
}

impl ScanSource<u32> for WideSource<'_> {
    fn getc(&mut self) -> Option<u32> {
        let value = self.data.get(self.position).copied();
        if value.is_some() {
            self.position = self.position.saturating_add(1);
        }
        value
    }

    fn ungetc(&mut self, _c: u32) {
        self.position = self.position.saturating_sub(1);
    }
}

fn wide_length(value: *const u32) -> usize {
    let mut length = 0usize;
    // SAFETY: the caller's string has a NUL; the walk cannot pass it.
    while unsafe { *value.add(length) } != 0 {
        length = length.wrapping_add(1);
    }
    length
}

/// `vswscanf`: scan a wide string.
///
/// # Safety
/// `input` and `format` are NUL-terminated; `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vswscanf(
    input: *const u32,
    format: *const u32,
    list: *mut List,
) -> c_int {
    if input.is_null() || format.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(22)));
    }
    // SAFETY: both strings are NUL-terminated.
    let input = unsafe { core::slice::from_raw_parts(input, wide_length(input)) };
    // SAFETY: both strings are NUL-terminated.
    let format = unsafe { core::slice::from_raw_parts(format, wide_length(format)) };
    let mut source = WideSource {
        data: input,
        position: 0,
    };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = ScanOptions::default();
    match vscan(&mut source, format, &mut env, &options, &crate::stdio::vsscanf::ScanFloat) {
        Ok(value) if value.return_eof => -1,
        Ok(value) => value.assigned,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
