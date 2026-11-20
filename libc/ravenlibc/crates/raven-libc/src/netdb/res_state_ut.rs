// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn state_pointer_is_stable_and_zeroed() {
    let first = __res_state();
    let second = __res_state();
    assert!(!first.is_null());
    assert_eq!(first, second);
    // SAFETY: the export owns a 2048-byte static state buffer.
    let bytes = unsafe { core::slice::from_raw_parts(first.cast::<u8>(), 64) };
    assert!(bytes.iter().all(|byte| *byte == 0));
}
