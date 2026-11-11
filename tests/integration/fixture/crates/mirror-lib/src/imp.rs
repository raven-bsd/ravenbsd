// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared implementation of the mirror fixture; the kernel includes this file.

/// Double a value.
pub fn double(value: u32) -> u32 {
    value * 2
}
