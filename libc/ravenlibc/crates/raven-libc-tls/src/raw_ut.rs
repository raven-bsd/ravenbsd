// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::consts::MAX_TLS_MODULES;

#[test]
fn builtin_buffer_is_aligned_and_zeroed() {
    assert_eq!(BUILTIN_TLS.as_ptr() as usize % 16, 0);
    assert_eq!(BUILTIN_TLS.capacity(), BUILTIN_TLS_SIZE);
    // The buffer is a zeroed static; its contents are checked through `copy_tls` in the
    // integration tests, where the test crate is allowed to perform the raw write.
    assert!(!BUILTIN_TLS.as_ptr().is_null());
}

#[test]
fn dtv_words_counts_the_header() {
    let modules = [TlsModuleIn::new(0x1000, 0x10, 0x10, 16)];
    assert_eq!(dtv_words(&modules), 2);
    assert_eq!(dtv_words(&[]), 1);
    let too_many = [TlsModuleIn::new(0, 0, 0, 1); MAX_TLS_MODULES + 4];
    assert_eq!(dtv_words(&too_many), MAX_TLS_MODULES + 1);
}

#[test]
fn abi_reports_the_target() {
    assert_eq!(abi(), crate::consts::TARGET_ABI);
}
