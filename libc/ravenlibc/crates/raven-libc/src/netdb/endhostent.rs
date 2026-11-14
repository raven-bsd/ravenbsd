// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `endhostent`.

use super::records;

/// `endhostent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn endhostent() {
    records::end_hosts();
}
