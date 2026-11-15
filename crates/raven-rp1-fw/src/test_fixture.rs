// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Test-only reader for the synthetic RP1 firmware fixture.

use raven_rp1::{Digest, FirmwareImage};

/// The checked-in synthetic fixture (not the real firmware).
pub(crate) const FIXTURE: &str = include_str!("../../../tests/fixtures/pi5/rp1-fw-synthetic.md");

fn field(name: &str) -> Option<&'static str> {
    let prefix = format!("{name}: ");
    FIXTURE
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
}

fn hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let mut index = 0;
    while index < text.len() {
        let pair = text.get(index..index + 2)?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
        index += 2;
    }
    Some(bytes)
}

pub(crate) fn blob() -> Vec<u8> {
    let data = field("data").expect("fixture data line");
    hex(data).expect("fixture data is hex")
}

pub(crate) fn image() -> FirmwareImage {
    let path = field("path").expect("fixture path line");
    let digest = Digest::from_hex(field("sha256").expect("fixture digest line")).expect("digest");
    let size = field("size")
        .expect("fixture size line")
        .parse::<u64>()
        .expect("size");
    let licence = field("licence").expect("fixture licence line");
    FirmwareImage::recorded(path, digest, size, licence)
}
