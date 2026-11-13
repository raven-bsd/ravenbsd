// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Locale model tests.

use super::*;

struct Env(&'static [(&'static [u8], &'static [u8])]);

impl EnvSource for Env {
    fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.0
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    }
}

fn empty_env() -> Env {
    Env(&[])
}

#[test]
fn initial_state_is_c_for_every_category() {
    let mut state = LocaleState::new();
    for cat in 0..=LC_ALL {
        assert_eq!(
            state.setlocale(cat, None, &empty_env()),
            Some(b"C".as_slice())
        );
    }
    assert_eq!(state.global(), Locale::C);
}

#[test]
fn utf8_names_are_stored_and_returned() {
    let mut state = LocaleState::new();
    for name in [
        b"C.UTF-8".as_slice(),
        b"en_US.UTF-8",
        b"de_DE.UTF-8",
        b"ru_RU.KOI8-R",
        b"xx_YY",
    ] {
        assert_eq!(
            state.setlocale(LC_CTYPE, Some(name), &empty_env()),
            Some(name)
        );
        assert!(state.global().ctype_is_utf8(state.table()));
    }
}

#[test]
fn posix_is_c_behaviour_but_keeps_its_name() {
    let mut state = LocaleState::new();
    assert_eq!(
        state.setlocale(LC_ALL, Some(b"POSIX"), &empty_env()),
        Some(b"POSIX".as_slice())
    );
    assert!(!state.global().ctype_is_utf8(state.table()));
    assert_eq!(state.global().mb_cur_max(state.table()), 1);
}

#[test]
fn a_name_too_long_or_with_slash_becomes_c_utf8() {
    let mut state = LocaleState::new();
    let long = [b'a'; 64];
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(&long), &empty_env()),
        Some(b"C.UTF-8".as_slice())
    );
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(b"en/US"), &empty_env()),
        Some(b"C.UTF-8".as_slice())
    );
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(b".hidden"), &empty_env()),
        Some(b"C.UTF-8".as_slice())
    );
}

#[test]
fn empty_name_reads_lc_all_then_category_then_lang() {
    let mut state = LocaleState::new();
    let env = Env(&[(b"LC_ALL", b"x"), (b"LC_CTYPE", b"y"), (b"LANG", b"z")]);
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(b""), &env),
        Some(b"x".as_slice())
    );
    let env = Env(&[(b"LC_CTYPE", b"y"), (b"LANG", b"z")]);
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(b""), &env),
        Some(b"y".as_slice())
    );
    let env = Env(&[(b"LANG", b"z")]);
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(b""), &env),
        Some(b"z".as_slice())
    );
    let env = Env(&[]);
    assert_eq!(
        state.setlocale(LC_CTYPE, Some(b""), &env),
        Some(b"C.UTF-8".as_slice())
    );
}

#[test]
fn category_seven_is_rejected() {
    let mut state = LocaleState::new();
    assert_eq!(state.setlocale(7, Some(b"C"), &empty_env()), None);
}

#[test]
fn mixed_categories_use_the_semicolon_composite() {
    let mut state = LocaleState::new();
    assert_eq!(
        state.setlocale(LC_TIME, Some(b"en_US.UTF-8"), &empty_env()),
        Some(b"en_US.UTF-8".as_slice())
    );
    let composite = state
        .setlocale(LC_ALL, None, &empty_env())
        .expect("composite")
        .to_vec();
    assert!(composite.contains(&b';'));
    let returned = state
        .setlocale(LC_ALL, Some(&composite), &empty_env())
        .expect("round trip")
        .to_vec();
    assert_eq!(returned, composite);
}

#[test]
fn newlocale_copies_the_base_and_overwrites_the_mask() {
    let mut state = LocaleState::new();
    let locale = state
        .newlocale(1 << LC_CTYPE, b"en_US.UTF-8", Some(Locale::C))
        .expect("newlocale");
    assert!(locale.ctype_is_utf8(state.table()));
    assert_eq!(state.table().name(locale.cat[LC_TIME]), b"C");
    let bad = state.newlocale(1 << 7, b"en_US.UTF-8", None);
    assert_eq!(bad, Err(LocaleError::Invalid));
    let empty = state.newlocale(LC_ALL_MASK, b"", None);
    assert_eq!(empty, Err(LocaleError::Invalid));
}

#[test]
fn uselocale_swaps_and_reports_the_previous() {
    let mut slot = LocaleArg::Global;
    let object = LocaleArg::Obj(Locale::CUTF8);
    assert_eq!(uselocale(&mut slot, Some(object)), LocaleArg::Global);
    assert_eq!(uselocale(&mut slot, None), object);
    assert_eq!(slot, object);
}

#[test]
fn duplocale_copies_the_global() {
    let mut state = LocaleState::new();
    state.setlocale(LC_TIME, Some(b"en_US.UTF-8"), &empty_env());
    assert_eq!(state.duplocale(LocaleArg::Global), state.global());
}

#[test]
fn mb_cur_max_is_one_only_for_c_and_posix() {
    let mut state = LocaleState::new();
    assert_eq!(state.global().mb_cur_max(state.table()), 1);
    state.setlocale(LC_CTYPE, Some(b"C.UTF-8"), &empty_env());
    assert_eq!(state.global().mb_cur_max(state.table()), 4);
    state.setlocale(LC_TIME, Some(b"C"), &empty_env());
    assert_eq!(state.global().mb_cur_max(state.table()), 4);
}

#[test]
fn the_257th_name_maps_to_c_utf8() {
    let mut table = NameTable::new();
    for index in 0..300 {
        let name = format!("name{index}");
        table.intern(name.as_bytes());
    }
    assert_eq!(table.name(NameId(1)), b"C.UTF-8");
    assert!(table.is_utf8(NameId(1)));
}

macro_rules! category_name_case {
    ($name:ident, $cat:expr, $value:expr) => {
        #[test]
        fn $name() {
            let mut state = LocaleState::new();
            let expected: &[u8] = $value;
            assert_eq!(
                state.setlocale($cat, Some(expected), &empty_env()),
                Some(expected)
            );
        }
    };
}

category_name_case!(cat_ctype_utf8, LC_CTYPE, b"en_US.UTF-8");
category_name_case!(cat_numeric_utf8, LC_NUMERIC, b"de_DE.UTF-8");
category_name_case!(cat_time_utf8, LC_TIME, b"fr_FR.UTF-8");
category_name_case!(cat_collate_utf8, LC_COLLATE, b"es_ES.UTF-8");
category_name_case!(cat_monetary_utf8, LC_MONETARY, b"it_IT.UTF-8");
category_name_case!(cat_messages_utf8, LC_MESSAGES, b"pt_BR.UTF-8");
category_name_case!(cat_ctype_c, LC_CTYPE, b"C");
category_name_case!(cat_numeric_posix, LC_NUMERIC, b"POSIX");
category_name_case!(cat_time_cutf8, LC_TIME, b"C.UTF-8");
category_name_case!(cat_collate_xx, LC_COLLATE, b"xx_YY");
