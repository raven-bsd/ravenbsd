// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `str-errmsg`: the Rust subjects against the donor C.

#![cfg(feature = "oracle-str-errmsg")]

use raven_libc_oracle::decls::str_common::StrCase;
use raven_libc_oracle::diff_case;

diff_case! {
    name: strerror,
    case: StrCase,
    fixed: "str/strerror/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strerror_oracle(),
    subject: |c| c.strerror_subject(),
}

diff_case! {
    name: strsignal,
    case: StrCase,
    fixed: "str/strsignal/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strsignal_oracle(),
    subject: |c| c.strsignal_subject(),
}
