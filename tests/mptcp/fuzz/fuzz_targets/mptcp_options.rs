// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Decode every MPTCP option in an arbitrary TCP option block. The target must return a
    // `Result` for every input and never panic; an accepted option must re-encode to exactly the
    // bytes it consumed.
    let _ = raven_mptcp::option::parse_options(data);
    if let Ok((option, used)) = raven_mptcp::MptcpOption::parse(data) {
        assert_eq!(option.to_vec(), &data[..used]);
    }
});
