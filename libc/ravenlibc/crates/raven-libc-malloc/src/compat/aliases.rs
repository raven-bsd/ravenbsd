// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The glibc `__libc_*` alias table for the C-ABI shell and `mkmap`.

/// Pairs of `(alias, target)` names; data only, no exported code.
pub const LIBC_ALIASES: [(&str, &str); 7] = [
    ("__libc_malloc", "malloc"),
    ("__libc_calloc", "calloc"),
    ("__libc_realloc", "realloc"),
    ("__libc_free", "free"),
    ("__libc_memalign", "memalign"),
    ("__libc_valloc", "valloc"),
    ("__libc_pvalloc", "pvalloc"),
];

#[cfg(test)]
#[path = "aliases_ut.rs"]
mod tests;
