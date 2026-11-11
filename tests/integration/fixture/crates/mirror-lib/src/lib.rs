// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-test wrapper: the real implementation is `imp.rs`, which the kernel includes directly.

#[path = "imp.rs"]
mod imp;

/// Double a value through the shared implementation.
pub fn double(value: u32) -> u32 {
    imp::double(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles() {
        assert_eq!(double(2), 4);
    }
}
