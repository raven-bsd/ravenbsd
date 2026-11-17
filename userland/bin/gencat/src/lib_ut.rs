// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the gencat compiler.

use raven_libc_locale::gettext::FileSource;
use raven_libc_locale::locale::EnvSource;
use raven_libc_locale::nls::Catalog;

use super::*;

struct Files {
    data: Vec<u8>,
}

impl FileSource for Files {
    fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>> {
        if path == b"/catalog" && self.data.len() <= max {
            Some(self.data.clone())
        } else {
            None
        }
    }
}

struct Empty;

impl EnvSource for Empty {
    fn get(&self, _name: &[u8]) -> Option<&[u8]> {
        None
    }
}

fn open(image: Vec<u8>) -> Catalog {
    Catalog::open(&Files { data: image }, &Empty, b"/catalog", 0, false, b"C").expect("catalog")
}

#[test]
fn compiles_a_readable_catalog() {
    let source = "$set 1\n1 \"Hello\"\n2 \"Line\\nNext\"\n$set 2\n7 \"Caf\\t\"\n";
    let image = compile(&[source]).expect("compile");
    assert_eq!(&image[..4], &[0xff, 0x88, 0xff, 0x89]);
    let catalog = open(image);
    assert_eq!(catalog.gets(1, 1, b"missing"), b"Hello");
    assert_eq!(catalog.gets(1, 2, b"missing"), b"Line\nNext");
    assert_eq!(catalog.gets(2, 7, b"missing"), b"Caf\t");
    assert_eq!(catalog.gets(1, 99, b"missing"), b"missing");
    assert_eq!(catalog.gets(9, 1, b"missing"), b"missing");
}

#[test]
fn delset_removes_a_set() {
    let source = "$set 1\n1 \"A\"\n$delset 1\n$set 2\n1 \"B\"\n";
    let catalog = open(compile(&[source]).expect("compile"));
    assert_eq!(catalog.gets(1, 1, b"missing"), b"missing");
    assert_eq!(catalog.gets(2, 1, b"missing"), b"B");
}

#[test]
fn source_files_share_the_current_set() {
    let catalog = open(compile(&["$set 3\n", "4 \"D\"\n"]).expect("compile"));
    assert_eq!(catalog.gets(3, 4, b"missing"), b"D");
}

#[test]
fn rejects_bad_source() {
    assert!(compile(&["1 \"before set\"\n"]).is_err());
    assert!(compile(&["$set x\n"]).is_err());
    assert!(compile(&["$set 1\n1 \"unterminated\n"]).is_err());
    assert!(compile(&["$set 1\n1 \"bad \\q\"\n"]).is_err());
    assert!(compile(&["$set 1\n0 \"zero\"\n"]).is_err());
    assert!(compile(&["$set 1\n1 \"ok\" trailing\n"]).is_err());
}
