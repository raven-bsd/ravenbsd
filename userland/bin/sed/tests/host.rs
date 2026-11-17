// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host end-to-end checks for the `sed` subset.

use raven_sed::run;

#[test]
fn address_range_and_substitution_session() {
    let output = run("1s/^/first: /\n2,3s/a/b/g\n$p", "a a\na a\na\n", true).unwrap();
    assert_eq!(output, "b\n");
}

#[test]
fn append_insert_change_and_translate() {
    let output = run(
        "1i\\\nbefore\n1a\\\nafter\n2c\\\nchanged\n3y/ab/xy/",
        "a\nb\nab\n",
        false,
    )
    .unwrap();
    assert_eq!(output, "before\na\nafter\nchanged\nxy\n");
}
