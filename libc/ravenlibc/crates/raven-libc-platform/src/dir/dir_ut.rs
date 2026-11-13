// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Directory module tests.

#[test]
fn getcwd_reexport_matches_the_implementation() {
    let mut buffer = [0u8; 4];
    let result = crate::dir::getcwd(b"/x", Some(&mut buffer), 4);
    assert_eq!(result, Ok(None));
    assert_eq!(&buffer[..3], b"/x\0");
}
