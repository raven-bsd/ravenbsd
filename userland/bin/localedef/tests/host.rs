// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host end-to-end checks: the emitted image is readable by the runtime catalog reader.

use raven_libc_locale::gettext::FileSource;
use raven_libc_locale::locale::EnvSource;
use raven_libc_locale::nls::Catalog;

struct Image(Vec<u8>);

impl FileSource for Image {
    fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>> {
        if path == b"/locale.cat" && self.0.len() <= max {
            Some(self.0.clone())
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

#[test]
fn a_compiled_locale_catalog_is_readable() {
    let source = "LC_NUMERIC\ndecimal_point \".\"\nthousands_sep \",\"\nEND LC_NUMERIC\n";
    let charmap = "<U002E> /x2e U+002E FULL STOP\n<U002C> /x2c U+002C COMMA\n";
    let image = localedef::compile(source, charmap, "UTF-8", Some("x")).expect("compile");
    assert_eq!(image.get(..4), Some([0xff, 0x88, 0xff, 0x89].as_slice()));

    let catalog =
        Catalog::open(&Image(image), &Empty, b"/locale.cat", 0, false, b"C").expect("open");
    assert_eq!(catalog.gets(2, 1, b"default"), &b"."[..]);
    assert_eq!(catalog.gets(2, 2, b"default"), &b","[..]);
    assert_eq!(catalog.gets(7, 1, b"default"), &b"x"[..]);
    assert_eq!(catalog.gets(7, 2, b"default"), &b"UTF-8"[..]);
    assert_eq!(catalog.gets(7, 3, b"default"), &b"2"[..]);
    assert_eq!(catalog.gets(7, 4, b"default"), &b"LC_NUMERIC"[..]);
}

#[test]
fn unsupported_source_produces_no_image() {
    let source = "LC_FOO\nx \"y\"\nEND LC_FOO\n";
    let charmap = "<U002E> /x2e U+002E FULL STOP\n";
    assert!(localedef::compile(source, charmap, "UTF-8", None).is_err());
}
