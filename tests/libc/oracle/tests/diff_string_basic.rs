// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `string-basic`: Rust stand-ins against the donor C.

#![cfg(feature = "oracle-string-basic")]

use raven_libc_oracle::call::{MemchrCase, StrlcpyCase, StrnlenCase};
use raven_libc_oracle::diff_case;
use raven_libc_oracle::standin;

diff_case! {
    name: strlcpy,
    case: StrlcpyCase,
    fixed: "strlcpy/basic.toml",
    random: StrlcpyCase::random,
    exhaustive: (small, StrlcpyCase::exhaustive),
    oracle: |c| c.run_oracle(),
    subject: |c| c.run_subject(standin::strlcpy::strlcpy),
}

diff_case! {
    name: strnlen,
    case: StrnlenCase,
    fixed: "strnlen/basic.toml",
    random: StrnlenCase::random,
    exhaustive: (small, StrnlenCase::exhaustive),
    oracle: |c| c.run_oracle(),
    subject: |c| c.run_subject(standin::strnlen::strnlen),
}

diff_case! {
    name: memchr,
    case: MemchrCase,
    fixed: "memchr/basic.toml",
    random: MemchrCase::random,
    exhaustive: (small, MemchrCase::exhaustive),
    oracle: |c| c.run_oracle(),
    subject: |c| c.run_subject(standin::memchr::memchr),
}
