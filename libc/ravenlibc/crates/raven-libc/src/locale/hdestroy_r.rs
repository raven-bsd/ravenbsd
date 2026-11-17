// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hdestroy_r`.

use super::search::HsearchData;

/// `hdestroy_r`.
///
/// # Safety
/// `data` is NULL or the table of a successful `hcreate_r`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hdestroy_r(data: *mut HsearchData) {
    super::search::remove(data);
}
