// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test: 200 specs x 16 modes x 4 umasks against the C original
//! (`openbsd:lib/libc/gen/setmode.c`), recorded by the C65 host probe (see
//! `tests/data/setmode_oracle.rs`). The C `setmode` reads the umask itself; the driver set it
//! before both calls, which is exactly the `getmode(omode, umask)` contract here.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[path = "data/setmode_oracle.rs"]
mod oracle;

use raven_libc_compat::setmode::setmode;

const MODES: [u32; 16] = [
    0, 0o1, 0o7, 0o10, 0o77, 0o100, 0o644, 0o700, 0o711, 0o744, 0o755, 0o770, 0o777, 0o1777,
    0o4755, 0o7777,
];
const UMASKS: [u32; 4] = [0, 0o022, 0o077, 0o777];

#[test]
fn two_hundred_specs_times_sixteen_modes_times_four_umasks() {
    assert_eq!(oracle::SPECS.len(), 200);
    let mut checked = 0usize;
    for (spec, expected) in oracle::SPECS {
        let parsed = setmode(spec.as_bytes());
        for (umask_index, umask) in UMASKS.iter().enumerate() {
            for (mode_index, omode) in MODES.iter().enumerate() {
                let want = expected[umask_index][mode_index];
                let got = parsed.as_ref().map(|set| set.getmode(*omode, *umask));
                assert_eq!(got, want, "spec {spec:?} omode {omode:o} umask {umask:o}");
                checked = checked.wrapping_add(1);
            }
        }
    }
    assert_eq!(checked, 200 * 16 * 4);
}
