// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
//! CJK table dimension and sample tests.

use raven_libc_iconv_cjk::*;

#[test]
fn dimensions_match_the_donor() {
    assert_eq!(JIS0208.len(), 84);
    assert_eq!(JIS0208[0].len(), 94);
    assert_eq!(GB18030.len(), 126);
    assert_eq!(GB18030[0].len(), 190);
    assert_eq!(BIG5.len(), 89);
    assert_eq!(BIG5[0].len(), 157);
    assert_eq!(KSC.len(), 93);
    assert_eq!(KSC[0].len(), 94);
    assert!(!REVJIS.is_empty());
    assert!(!HKSCS.is_empty());
    assert!(!GB18030UTF.is_empty());
}

macro_rules! cell_case {
    ($name:ident, $table:expr, $row:expr, $column:expr) => {
        #[test]
        fn $name() {
            assert_ne!($table[$row][$column], 0);
        }
    };
}

cell_case!(jis_00, JIS0208, 0, 0);
cell_case!(jis_01, JIS0208, 0, 1);
cell_case!(jis_02, JIS0208, 0, 2);
cell_case!(jis_03, JIS0208, 0, 10);
cell_case!(jis_04, JIS0208, 1, 0);
cell_case!(jis_05, JIS0208, 0, 3);
cell_case!(jis_06, JIS0208, 3, 0);
cell_case!(jis_07, JIS0208, 4, 0);
cell_case!(gb_00, GB18030, 0, 0);
cell_case!(gb_01, GB18030, 0, 1);
cell_case!(gb_02, GB18030, 0, 10);
cell_case!(gb_03, GB18030, 1, 0);
cell_case!(gb_04, GB18030, 2, 0);
cell_case!(gb_05, GB18030, 3, 0);
cell_case!(big5_00, BIG5, 0, 0);
cell_case!(big5_01, BIG5, 0, 1);
cell_case!(big5_02, BIG5, 0, 10);
cell_case!(big5_03, BIG5, 1, 0);
cell_case!(big5_04, BIG5, 2, 0);
cell_case!(big5_05, BIG5, 3, 0);
cell_case!(ksc_00, KSC, 0, 0);
cell_case!(ksc_01, KSC, 0, 1);
cell_case!(ksc_02, KSC, 0, 10);
cell_case!(ksc_03, KSC, 1, 0);
cell_case!(ksc_04, KSC, 2, 0);
cell_case!(ksc_05, KSC, 3, 0);

macro_rules! flat_case {
    ($name:ident, $table:expr, $index:expr, $value:expr) => {
        #[test]
        fn $name() {
            assert_eq!($table[$index], $value);
        }
    };
}

flat_case!(revjis_0, REVJIS, 0, 31);
flat_case!(revjis_1, REVJIS, 1, 80);
flat_case!(revjis_2, REVJIS, 2, 81);

#[test]
fn all_cjk_tables_fit_the_budget() {
    let bytes = core::mem::size_of_val(&JIS0208)
        + core::mem::size_of_val(&GB18030)
        + core::mem::size_of_val(&GB18030UTF)
        + core::mem::size_of_val(&BIG5)
        + core::mem::size_of_val(&HKSCS)
        + core::mem::size_of_val(&KSC)
        + core::mem::size_of_val(&REVJIS);
    assert!(bytes <= 140 * 1024, "CJK tables {bytes} bytes");
}
