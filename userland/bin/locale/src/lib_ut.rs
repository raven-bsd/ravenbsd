// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for locale reporting.

use alloc::collections::BTreeMap;

use super::*;

#[derive(Default)]
struct MapEnv {
    values: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl MapEnv {
    fn new(pairs: &[(&str, &str)]) -> Self {
        let mut values = BTreeMap::new();
        for (name, value) in pairs {
            values.insert(name.as_bytes().to_vec(), value.as_bytes().to_vec());
        }
        Self { values }
    }
}

impl EnvSource for MapEnv {
    fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.values.get(name).map(Vec::as_slice)
    }
}

#[test]
fn c_locale_reports_ascii_codeset() {
    let env = MapEnv::new(&[("LANG", "C")]);
    let mut state = resolve(&env);
    assert_eq!(category_value(&mut state, LC_CTYPE), "C");
    assert_eq!(codeset(&state), "ANSI_X3.4-1968");
    let report = render(&env);
    assert!(report.contains("LANG=C\n"));
    assert!(report.contains("LC_CTYPE=\"C\"\n"));
    assert!(report.contains("codeset=ANSI_X3.4-1968\n"));
}

#[test]
fn utf8_locale_reports_utf8_codeset() {
    let env = MapEnv::new(&[("LANG", "C.UTF-8")]);
    let mut state = resolve(&env);
    assert_eq!(category_value(&mut state, LC_CTYPE), "C.UTF-8");
    assert_eq!(codeset(&state), "UTF-8");
}

#[test]
fn lc_all_overrides_lang() {
    let env = MapEnv::new(&[("LANG", "C.UTF-8"), ("LC_ALL", "C")]);
    let mut state = resolve(&env);
    assert_eq!(category_value(&mut state, LC_CTYPE), "C");
    assert_eq!(codeset(&state), "ANSI_X3.4-1968");
}

#[test]
fn missing_environment_defaults_to_utf8() {
    let env = MapEnv::default();
    let mut state = resolve(&env);
    assert_eq!(category_value(&mut state, LC_CTYPE), "C.UTF-8");
    assert_eq!(codeset(&state), "UTF-8");
}

#[test]
fn queries_named_categories() {
    let env = MapEnv::new(&[("LANG", "C")]);
    let mut state = resolve(&env);
    assert_eq!(
        query(&mut state, &env, "LC_MESSAGES").expect("category"),
        "LC_MESSAGES=\"C\""
    );
    assert_eq!(query(&mut state, &env, "LANG").expect("lang"), "LANG=C");
    assert!(query(&mut state, &env, "LC_UNKNOWN").is_err());
}

#[test]
fn composite_lc_all_is_reported() {
    let env = MapEnv::new(&[("LANG", "C"), ("LC_TIME", "C.UTF-8")]);
    let mut state = resolve(&env);
    let value = category_value(&mut state, LC_ALL);
    assert!(value.contains(";C.UTF-8;"));
}
