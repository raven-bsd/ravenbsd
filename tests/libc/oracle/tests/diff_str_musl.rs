// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `str-musl`: the Rust subjects against the donor C.

#![cfg(feature = "oracle-str-musl")]

use raven_libc_oracle::decls::str_common::StrCase;
use raven_libc_oracle::diff_case;

diff_case! {
    name: strlen,
    case: StrCase,
    fixed: "str/strlen/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strlen_oracle(),
    subject: |c| c.strlen_subject(),
}

diff_case! {
    name: strnlen,
    case: StrCase,
    fixed: "str/strnlen/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strnlen_oracle(),
    subject: |c| c.strnlen_subject(),
}

diff_case! {
    name: strchr,
    case: StrCase,
    fixed: "str/strchr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strchr_oracle(),
    subject: |c| c.strchr_subject(),
}

diff_case! {
    name: strrchr,
    case: StrCase,
    fixed: "str/strrchr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strrchr_oracle(),
    subject: |c| c.strrchr_subject(),
}

diff_case! {
    name: strchrnul,
    case: StrCase,
    fixed: "str/strchrnul/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strchrnul_oracle(),
    subject: |c| c.strchrnul_subject(),
}

diff_case! {
    name: stpcpy,
    case: StrCase,
    fixed: "str/stpcpy/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.stpcpy_oracle(),
    subject: |c| c.stpcpy_subject(),
}

diff_case! {
    name: stpncpy,
    case: StrCase,
    fixed: "str/stpncpy/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.stpncpy_oracle(),
    subject: |c| c.stpncpy_subject(),
}

diff_case! {
    name: strdup,
    case: StrCase,
    fixed: "str/strdup/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strdup_oracle(),
    subject: |c| c.strdup_subject(),
}

diff_case! {
    name: strndup,
    case: StrCase,
    fixed: "str/strndup/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strndup_oracle(),
    subject: |c| c.strndup_subject(),
}
