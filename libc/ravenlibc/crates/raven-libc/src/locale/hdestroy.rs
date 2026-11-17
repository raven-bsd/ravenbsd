// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `hdestroy`.

/// `hdestroy`.
#[unsafe(no_mangle)]
pub extern "C" fn hdestroy() {
    // SAFETY: the global slot is the one `hcreate` wrote.
    let table = unsafe { (*super::search::GLOBAL.0.get()).take() };
    drop(table);
}
