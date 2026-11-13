// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// SPDX-License-Identifier: BSD-3-Clause

//! Independent 8-bit map oracle: parse the NetBSD csmapper `.src` text when present.

use std::path::Path;

#[test]
fn every_iso8859_1_byte_matches_the_csmapper_source() {
    let root = std::env::var_os("RAVEN_CSMAPPER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(
                "/home/boma6672/work/research/raven/ravenbsd-resources/netbsd/share/i18n/csmapper",
            )
            .to_path_buf()
        });
    let path = root.join("ISO-8859/ISO-8859-1%UCS.src");
    let Ok(text) = std::fs::read_to_string(&path) else {
        println!("csmapper: skipped: directory absent ({})", path.display());
        return;
    };
    let mut map = [0xfffeu32; 256];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let Ok(byte) = u8::from_str_radix(left.trim().trim_start_matches("0x"), 16) else {
            continue;
        };
        let Ok(value) = u32::from_str_radix(right.trim().trim_start_matches("0x"), 16) else {
            continue;
        };
        map[byte as usize] = value;
    }
    let mut compared = 0usize;
    for byte in 0u8..=0xff {
        let expected = map[byte as usize];
        if expected == 0xfffe || expected > 0x10ffff {
            continue;
        }
        let mut converter = raven_libc_iconv::Iconv::open(b"UTF-8", b"ISO-8859-1").expect("open");
        let mut input = [byte];
        let mut source: &[u8] = &mut input;
        let mut buffer = [0u8; 8];
        let mut output: &mut [u8] = &mut buffer;
        converter.convert(&mut source, &mut output).expect("decode");
        let used = 8 - output.len();
        let decoded = std::str::from_utf8(&buffer[..used]).expect("utf8");
        assert_eq!(decoded.chars().next().map(|ch| ch as u32), Some(expected));
        compared += 1;
    }
    println!("csmapper: ISO-8859-1 {compared} bytes compared");
}
