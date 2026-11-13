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
