// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(error) = raven_fs_fuzz_replay::replay("ufs_superblock", data) {
        panic!("{error}");
    }
});
