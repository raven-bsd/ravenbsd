// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the variadic table (`table.rs`).

use super::*;

/// Parse check: every row must have a literal export, a path target and an in-range `named`.
macro_rules! check_row {
    (prefix: $prefix:literal, export: $export:literal, target: $target:path, named: $n:literal) => {
        #[allow(dead_code)]
        const _: &str = $export;
        const _: () = assert!($n <= crate::va::MAX_NAMED_OTHER);
        const _: () = assert!($n >= 1);
    };
}

crate::for_each_variadic!(check_row, "t_");

#[test]
fn the_table_has_the_forty_rows_of_the_ticket() {
    assert_eq!(VARIADIC_ROWS, 40);
}

#[test]
fn the_largest_named_count_fits_x86_64() {
    // `strfmon_l` and `mremap` have four named arguments; every row must stay at or below
    // the x86_64 bound (5), otherwise the shell cannot place `&va_list`.
    assert!(4 <= crate::va::MAX_NAMED_X86_64);
}

#[test]
fn scanf_trampolines_use_the_canonical_named_counts() {
    // The scanf family is expanded by hand in `stdio/trampolines_*`; a named count that
    // differs from `for_each_variadic!` shifts the `va_list` and makes the target write
    // through the next argument (C74u: `sscanf` declared 3 instead of 2).
    let narrow = include_str!("../stdio/trampolines_scanf.rs");
    let wide = include_str!("../stdio/trampolines_wide_scan.rs");
    let expected = [
        ("scanf", 1usize),
        ("fscanf", 2),
        ("sscanf", 2),
        ("wscanf", 1),
        ("fwscanf", 2),
        ("swscanf", 2),
    ];
    for (export, named) in expected {
        let needle = alloc::format!("export: \"{export}\"");
        let line = narrow
            .lines()
            .chain(wide.lines())
            .find(|line| line.contains(needle.as_str()))
            .unwrap_or_else(|| panic!("missing trampoline for {export}"));
        let wanted = alloc::format!("named: {named}");
        assert!(
            line.contains(wanted.as_str()),
            "{export}: expected `{wanted}` in `{line}`"
        );
    }
}
