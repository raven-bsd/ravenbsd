// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `str-mem`: the Rust subjects against the donor C.

#![cfg(feature = "oracle-str-mem")]

use raven_libc_oracle::decls::str_common::StrCase;
use raven_libc_oracle::diff_case;

diff_case! {
    name: memcpy,
    case: StrCase,
    fixed: "str/memcpy/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memcpy_oracle(),
    subject: |c| c.memcpy_subject(),
}

diff_case! {
    name: memmove,
    case: StrCase,
    fixed: "str/memmove/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memmove_oracle(),
    subject: |c| c.memmove_subject(),
}

diff_case! {
    name: memset,
    case: StrCase,
    fixed: "str/memset/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memset_oracle(),
    subject: |c| c.memset_subject(),
}

diff_case! {
    name: memcmp,
    case: StrCase,
    fixed: "str/memcmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memcmp_oracle(),
    subject: |c| c.memcmp_subject(),
}

diff_case! {
    name: memchr,
    case: StrCase,
    fixed: "str/memchr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memchr_oracle(),
    subject: |c| c.memchr_subject(),
}
