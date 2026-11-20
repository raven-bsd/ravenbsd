// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K165: the `AT_RAVEN_CWD` auxv string lookup.

use super::*;

#[test]
fn aux_string_reads_a_loader_string() {
    let mut strings: Vec<u8> = Vec::new();
    strings.extend_from_slice(b"/bin\0");
    let text = strings.as_ptr() as usize;
    let words: Vec<usize> = vec![0, 0, 0, crate::spawn_attrs::AT_RAVEN_CWD, text, 0, 0];
    // SAFETY: the words form a well-formed C8 stack (argc, argv NULL, env NULL, auxv, AT_NULL).
    let args = unsafe { parse_stack(words.as_ptr()) };
    assert_eq!(
        args.aux_string(crate::spawn_attrs::AT_RAVEN_CWD),
        Some("/bin")
    );
    assert_eq!(args.aux_string(crate::spawn_attrs::AT_RAVEN_ABI), None);
}
