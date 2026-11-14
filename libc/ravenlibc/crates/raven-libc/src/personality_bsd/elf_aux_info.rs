// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `elf_aux_info(3)`.
use core::ffi::{c_int, c_void};

use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::{aux_lookup, ret_unit};

/// Copy one auxiliary-vector value into `buf`.
///
/// # Safety
/// `buf` is NULL with `buflen == 0`, or points to `buflen` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn elf_aux_info(aux: c_int, buf: *mut c_void, buflen: c_int) -> c_int {
    if buflen < 0 {
        return ret_unit(Err(Errno::EINVAL));
    }
    let length = usize::try_from(buflen).unwrap_or(0);
    if buf.is_null() {
        if length != 0 {
            return ret_unit(Err(Errno::EFAULT));
        }
        return ret_unit(raven_libc_personality_bsd::aux::elf_aux_info(
            aux,
            &mut [],
            &aux_lookup,
        ));
    }
    // SAFETY: the caller granted `buflen` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(buf.cast::<u8>(), length) };
    ret_unit(raven_libc_personality_bsd::aux::elf_aux_info(
        aux,
        buffer,
        &aux_lookup,
    ))
}
