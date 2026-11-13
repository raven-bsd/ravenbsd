// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `str-freebsd`: the Rust subjects against the donor C.

#![cfg(feature = "oracle-str-freebsd")]

use raven_libc_oracle::decls::str_common::StrCase;
use raven_libc_oracle::diff_case;

diff_case! {
    name: strnstr,
    case: StrCase,
    fixed: "str/strnstr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strnstr_oracle(),
    subject: |c| c.strnstr_subject(),
}

diff_case! {
    name: strverscmp,
    case: StrCase,
    fixed: "str/strverscmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strverscmp_oracle(),
    subject: |c| c.strverscmp_subject(),
}

diff_case! {
    name: mempcpy,
    case: StrCase,
    fixed: "str/mempcpy/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.mempcpy_oracle(),
    subject: |c| c.mempcpy_subject(),
}

diff_case! {
    name: memset_s,
    case: StrCase,
    fixed: "str/memset_s/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memset_s_oracle(),
    subject: |c| c.memset_s_subject(),
}

diff_case! {
    name: fls,
    case: StrCase,
    fixed: "str/fls/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.fls_oracle(),
    subject: |c| c.fls_subject(),
}

diff_case! {
    name: flsl,
    case: StrCase,
    fixed: "str/flsl/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.flsl_oracle(),
    subject: |c| c.flsl_subject(),
}

diff_case! {
    name: stdc_count_ones_uc,
    case: StrCase,
    fixed: "str/stdc_count_ones_uc/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.stdc_count_ones_uc_oracle(),
    subject: |c| c.stdc_count_ones_uc_subject(),
}

diff_case! {
    name: stdc_bit_ceil_ui,
    case: StrCase,
    fixed: "str/stdc_bit_ceil_ui/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.stdc_bit_ceil_ui_oracle(),
    subject: |c| c.stdc_bit_ceil_ui_subject(),
}
