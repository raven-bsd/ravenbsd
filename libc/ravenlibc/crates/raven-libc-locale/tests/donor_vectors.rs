// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
//! Public-API vectors for the locale crate.

use raven_libc_locale::casecmp::{wcscasecmp, wcsncasecmp};
use raven_libc_locale::collate::{strcoll, strxfrm, wcscoll, wcsxfrm};
use raven_libc_locale::gettext::plural_eval;
use raven_libc_locale::langinfo::*;
use raven_libc_locale::locale::*;
use raven_libc_locale::localeconv::localeconv;
use raven_libc_locale::wstring::*;

struct Empty;
impl EnvSource for Empty {
    fn get(&self, _name: &[u8]) -> Option<&[u8]> {
        None
    }
}

macro_rules! name_case {
    ($name:ident, $cat:expr, $value:expr) => {
        #[test]
        fn $name() {
            let mut state = LocaleState::new();
            let value: &[u8] = $value;
            assert_eq!(state.setlocale($cat, Some(value), &Empty), Some(value));
        }
    };
}

name_case!(ctype_en, LC_CTYPE, b"en_US.UTF-8");
name_case!(ctype_de, LC_CTYPE, b"de_DE.UTF-8");
name_case!(ctype_fr, LC_CTYPE, b"fr_FR.UTF-8");
name_case!(ctype_ru, LC_CTYPE, b"ru_RU.KOI8-R");
name_case!(ctype_c, LC_CTYPE, b"C");
name_case!(ctype_posix, LC_CTYPE, b"POSIX");
name_case!(ctype_cutf8, LC_CTYPE, b"C.UTF-8");
name_case!(ctype_utf8_suffix, LC_CTYPE, b"x.utf8");
name_case!(numeric_de, LC_NUMERIC, b"de_DE.UTF-8");
name_case!(numeric_c, LC_NUMERIC, b"C");
name_case!(numeric_posix, LC_NUMERIC, b"POSIX");
name_case!(time_fr, LC_TIME, b"fr_FR.UTF-8");
name_case!(time_c, LC_TIME, b"C");
name_case!(time_cutf8, LC_TIME, b"C.UTF-8");
name_case!(collate_es, LC_COLLATE, b"es_ES.UTF-8");
name_case!(collate_c, LC_COLLATE, b"C");
name_case!(monetary_it, LC_MONETARY, b"it_IT.UTF-8");
name_case!(monetary_c, LC_MONETARY, b"C");
name_case!(messages_pt, LC_MESSAGES, b"pt_BR.UTF-8");
name_case!(messages_c, LC_MESSAGES, b"C");

#[test]
fn initial_c_categories() {
    let mut state = LocaleState::new();
    for cat in 0..=LC_ALL {
        assert_eq!(state.setlocale(cat, None, &Empty), Some(b"C".as_slice()));
    }
}

#[test]
fn c_utf8_round_trip() {
    let mut state = LocaleState::new();
    assert_eq!(
        state.setlocale(LC_ALL, Some(b"C.UTF-8"), &Empty),
        Some(b"C.UTF-8".as_slice())
    );
    assert_eq!(state.global(), Locale::CUTF8);
    assert_eq!(state.global().mb_cur_max(state.table()), 4);
}

#[test]
fn composite_round_trip() {
    let mut state = LocaleState::new();
    let _ = state.setlocale(LC_TIME, Some(b"en_US.UTF-8"), &Empty);
    let composite = state
        .setlocale(LC_ALL, None, &Empty)
        .expect("query")
        .to_vec();
    let again = state
        .setlocale(LC_ALL, Some(&composite), &Empty)
        .expect("set")
        .to_vec();
    assert_eq!(composite, again);
}

#[test]
fn newlocale_mask_and_error() {
    let mut state = LocaleState::new();
    let value = state
        .newlocale(1 << LC_TIME, b"en_US.UTF-8", None)
        .expect("newlocale");
    assert!(!value.ctype_is_utf8(state.table()));
    assert_eq!(
        state.newlocale(1 << 7, b"C", None),
        Err(LocaleError::Invalid)
    );
}

#[test]
fn uselocale_global_and_object() {
    let mut slot = LocaleArg::Global;
    assert_eq!(
        uselocale(&mut slot, Some(LocaleArg::Obj(Locale::CUTF8))),
        LocaleArg::Global
    );
    assert_eq!(uselocale(&mut slot, None), LocaleArg::Obj(Locale::CUTF8));
}

#[test]
fn localeconv_static_values() {
    let value = localeconv();
    assert_eq!(value.decimal_point, b".\0");
    assert_eq!(value.thousands_sep, b"\0");
    assert_eq!(value.int_frac_digits, 127);
}

#[test]
fn codeset_items() {
    let table = NameTable::new();
    assert_eq!(
        nl_langinfo(CODESET, &Locale::C, &table),
        b"ANSI_X3.4-1968\0"
    );
    assert_eq!(nl_langinfo(CODESET, &Locale::CUTF8, &table), b"UTF-8\0");
}

macro_rules! langinfo_case {
    ($name:ident, $item:expr, $value:expr) => {
        #[test]
        fn $name() {
            let table = NameTable::new();
            assert_eq!(nl_langinfo($item, &Locale::C, &table), $value);
        }
    };
}

langinfo_case!(abday1, ABDAY_1, b"Sun\0");
langinfo_case!(abday7, ABDAY_7, b"Sat\0");
langinfo_case!(day1, DAY_1, b"Sunday\0");
langinfo_case!(mon1, MON_1, b"January\0");
langinfo_case!(abmon12, ABMON_12, b"Dec\0");
langinfo_case!(am, AM_STR, b"AM\0");
langinfo_case!(pm, PM_STR, b"PM\0");
langinfo_case!(d_t_fmt, D_T_FMT, b"%a %b %e %T %Y\0");
langinfo_case!(d_fmt, D_FMT, b"%m/%d/%y\0");
langinfo_case!(t_fmt, T_FMT, b"%H:%M:%S\0");
langinfo_case!(radix, RADIXCHAR, b".\0");
langinfo_case!(thousands, THOUSEP, b"\0");
langinfo_case!(yesexpr, YESEXPR, b"^[yY]\0");
langinfo_case!(noexpr, NOEXPR, b"^[nN]\0");

macro_rules! collate_case {
    ($name:ident, $left:expr, $right:expr, $sign:expr) => {
        #[test]
        fn $name() {
            let result = strcoll($left, $right);
            assert_eq!(result.signum(), $sign);
        }
    };
}

collate_case!(collate_01, b"a", b"a", 0);
collate_case!(collate_02, b"a", b"b", -1);
collate_case!(collate_03, b"b", b"a", 1);
collate_case!(collate_04, b"a", b"aa", -1);
collate_case!(collate_05, &[0xff], &[0x01], 1);
collate_case!(collate_06, &[0x80], &[0x7f], 1);

#[test]
fn strxfrm_none_and_small() {
    assert_eq!(strxfrm(None, b"abc"), 3);
    let mut buffer = [0u8; 2];
    assert_eq!(strxfrm(Some(&mut buffer), b"abc"), 3);
    assert_eq!(buffer, [0; 2]);
}

#[test]
fn wide_collate() {
    assert!(wcscoll(&[0x8000_0000], &[1]) > 0);
    let mut buffer = [0u32; 3];
    assert_eq!(wcsxfrm(Some(&mut buffer), &[1, 2]), 2);
    assert_eq!(buffer, [1, 2, 0]);
}

macro_rules! wstring_case {
    ($name:ident, $expr:expr) => {
        #[test]
        fn $name() {
            assert!($expr);
        }
    };
}

wstring_case!(wcslen_empty, wcslen(&[]) == 0);
wstring_case!(wcslen_three, wcslen(&[1, 2, 3]) == 3);
wstring_case!(wcscmp_equal, wcscmp(&[1, 2], &[1, 2]) == 0);
wstring_case!(wcscmp_less, wcscmp(&[1], &[2]) < 0);
wstring_case!(wcschr_found, wcschr(&[1, 2], 2) == Some(1));
wstring_case!(wcschr_missing, wcschr(&[1, 2], 3).is_none());
wstring_case!(wcscspn_zero, wcscspn(&[1, 2], &[1]) == 0);
wstring_case!(wcsspn_two, wcsspn(&[1, 2, 3], &[1, 2]) == 2);
wstring_case!(wcsstr_found, wcsstr(&[1, 2, 3], &[2, 3]) == Some(1));
wstring_case!(wcsstr_empty, wcsstr(&[1], &[]) == Some(0));
wstring_case!(wmemchr_found, wmemchr(&[1, 2], 2) == Some(1));
wstring_case!(wmemcmp_unsigned, wmemcmp(&[0x8000_0000], &[1], 1) > 0);

#[test]
fn wide_casecmp() {
    assert_eq!(wcscasecmp(&[b'A' as u32], &[b'a' as u32]), 0);
    assert_eq!(wcsncasecmp(&[b'A' as u32], &[b'a' as u32], 1), 0);
}

macro_rules! plural_case {
    ($name:ident, $expr:expr, $n:expr, $value:expr) => {
        #[test]
        fn $name() {
            assert_eq!(plural_eval($expr, $n), Some($value));
        }
    };
}

plural_case!(plural_01, b"n!=1", 1, 0);
plural_case!(plural_02, b"n!=1", 2, 1);
plural_case!(plural_03, b"n>1", 1, 0);
plural_case!(plural_04, b"n>1", 2, 1);
plural_case!(plural_05, b"0", 9, 0);
plural_case!(plural_06, b"1", 9, 1);
plural_case!(plural_07, b"n==1?0:1", 1, 0);
plural_case!(plural_08, b"n==1?0:1", 2, 1);

#[test]
fn plural_division_by_zero() {
    assert_eq!(plural_eval(b"1/0", 1), None);
}
