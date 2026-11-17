// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fpathconf`.

use super::*;

/// Read a path configuration value.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fpathconf(fd: c_int, name: c_int) -> c_long {
    match global().fpathconf(fd, name) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => ret::long(Err(map_error(error))),
    }
}
