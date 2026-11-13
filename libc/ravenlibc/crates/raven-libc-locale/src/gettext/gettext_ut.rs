// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! gettext tests.

use super::*;
use crate::locale::{EnvSource, LC_MESSAGES, LocaleState};

use alloc::collections::BTreeMap;

struct Fs(BTreeMap<Vec<u8>, Vec<u8>>);
impl FileSource for Fs {
    fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>> {
        self.0.get(path).filter(|data| data.len() <= max).cloned()
    }
}

struct Env(&'static [(&'static [u8], &'static [u8])]);
impl EnvSource for Env {
    fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.0
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    }
}

fn mo(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
    let n = entries.len();
    let orig = 20usize;
    let trans = orig + 8 * n;
    let strings = trans + 8 * n;
    let mut data = vec![0u8; strings];
    let put = |data: &mut Vec<u8>, at: usize, value: u32| {
        if let Some(slot) = data.get_mut(at..at + 4) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    };
    put(&mut data, 0, 0x9504_12de);
    put(&mut data, 8, n as u32);
    put(&mut data, 12, orig as u32);
    put(&mut data, 16, trans as u32);
    for (index, (key, value)) in entries.iter().enumerate() {
        let key_at = data.len();
        data.extend_from_slice(key);
        data.push(0);
        let value_at = data.len();
        data.extend_from_slice(value);
        data.push(0);
        put(&mut data, orig + index * 8, key.len() as u32);
        put(&mut data, orig + index * 8 + 4, key_at as u32);
        put(&mut data, trans + index * 8, value.len() as u32);
        put(&mut data, trans + index * 8 + 4, value_at as u32);
    }
    data
}

fn locale() -> (LocaleState, Env) {
    let mut state = LocaleState::new();
    let _ = state.setlocale(LC_MESSAGES, Some(b"en_US.UTF-8"), &Env(&[]));
    (state, Env(&[]))
}

#[test]
fn translated_singular_and_plural() {
    let data = mo(&[
        (b"", b"Plural-Forms: nplurals=2; plural=n!=1;\0"),
        (b"file", b"fichier\0fichiers"),
    ]);
    let mut fs = BTreeMap::new();
    fs.insert(
        b"/usr/share/locale/en_US.UTF-8/LC_MESSAGES/messages.mo".to_vec(),
        data,
    );
    let fs = Fs(fs);
    let (state, env) = locale();
    let mut gettext = Gettext::new();
    let singular = gettext.dcngettext(
        &fs,
        &env,
        None,
        b"file",
        b"files",
        1,
        LC_MESSAGES,
        &state.global(),
        state.table(),
    );
    assert_eq!(singular, b"fichier");
    let plural = gettext.dcngettext(
        &fs,
        &env,
        None,
        b"file",
        b"files",
        2,
        LC_MESSAGES,
        &state.global(),
        state.table(),
    );
    assert_eq!(plural, b"fichiers");
}

#[test]
fn untranslated_returns_the_input_slice() {
    let fs = Fs(BTreeMap::new());
    let (state, env) = locale();
    let mut gettext = Gettext::new();
    let result = gettext.dcngettext(
        &fs,
        &env,
        None,
        b"missing",
        b"missing2",
        2,
        LC_MESSAGES,
        &state.global(),
        state.table(),
    );
    assert_eq!(result.as_ptr(), b"missing2".as_ptr());
}

#[test]
fn c_locale_skips_catalogs() {
    let data = mo(&[(b"file", b"fichier")]);
    let mut fs = BTreeMap::new();
    fs.insert(
        b"/usr/share/locale/en_US.UTF-8/LC_MESSAGES/messages.mo".to_vec(),
        data,
    );
    let fs = Fs(fs);
    let state = LocaleState::new();
    let mut gettext = Gettext::new();
    let result = gettext.dcngettext(
        &fs,
        &Env(&[]),
        None,
        b"file",
        b"files",
        1,
        LC_MESSAGES,
        &state.global(),
        state.table(),
    );
    assert_eq!(result, b"file");
}

#[test]
fn domains_and_codesets_are_stored() {
    let mut gettext = Gettext::new();
    assert_eq!(gettext.textdomain(Some(b"app")), Some(b"app".as_slice()));
    assert_eq!(
        gettext.bindtextdomain(b"app", Some(b"/tmp/locale")),
        Some(b"/tmp/locale".as_slice())
    );
    assert_eq!(
        gettext.bind_textdomain_codeset(b"app", Some(b"UTF-8")),
        Some(b"UTF-8".as_slice())
    );
}
