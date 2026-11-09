// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Random-text robustness tests for the manifest and override parsers.

use raven_svc_manifest::{Manifest, parse_overrides};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }
}

#[test]
fn random_byte_strings_never_panic() {
    let mut rng = Rng(0x5eed_1234_abcd_9876);
    for _ in 0..10_000 {
        let length = rng.below(200);
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            bytes.push((rng.next() & 0xff) as u8);
        }
        let text = String::from_utf8_lossy(&bytes);
        let _ = Manifest::parse(&text);
        let _ = parse_overrides(&text);
    }
}
