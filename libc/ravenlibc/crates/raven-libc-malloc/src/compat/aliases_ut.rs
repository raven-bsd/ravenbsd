// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn alias_table_has_the_glibc_names() {
    assert_eq!(LIBC_ALIASES.len(), 7);
    assert_eq!(LIBC_ALIASES[0], ("__libc_malloc", "malloc"));
    assert_eq!(LIBC_ALIASES[6], ("__libc_pvalloc", "pvalloc"));
}

#[test]
fn aliases_are_unique() {
    let mut index = 0usize;
    while index < LIBC_ALIASES.len() {
        let mut other = index + 1;
        while other < LIBC_ALIASES.len() {
            assert_ne!(LIBC_ALIASES[index].0, LIBC_ALIASES[other].0);
            other += 1;
        }
        index += 1;
    }
}
