// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `str-openbsd`: the Rust subjects against the donor C.

#![cfg(feature = "oracle-str-openbsd")]

use raven_libc_oracle::decls::str_common::StrCase;
use raven_libc_oracle::diff_case;

diff_case! {
    name: strcmp,
    case: StrCase,
    fixed: "str/strcmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strcmp_oracle(),
    subject: |c| c.strcmp_subject(),
}

diff_case! {
    name: strncmp,
    case: StrCase,
    fixed: "str/strncmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strncmp_oracle(),
    subject: |c| c.strncmp_subject(),
}

diff_case! {
    name: strcasecmp,
    case: StrCase,
    fixed: "str/strcasecmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strcasecmp_oracle(),
    subject: |c| c.strcasecmp_subject(),
}

diff_case! {
    name: strlcpy,
    case: StrCase,
    fixed: "str/strlcpy/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strlcpy_oracle(),
    subject: |c| c.strlcpy_subject(),
}

diff_case! {
    name: strlcat,
    case: StrCase,
    fixed: "str/strlcat/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strlcat_oracle(),
    subject: |c| c.strlcat_subject(),
}

diff_case! {
    name: strncat,
    case: StrCase,
    fixed: "str/strncat/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strncat_oracle(),
    subject: |c| c.strncat_subject(),
}

diff_case! {
    name: memrchr,
    case: StrCase,
    fixed: "str/memrchr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.memrchr_oracle(),
    subject: |c| c.memrchr_subject(),
}

diff_case! {
    name: strstr,
    case: StrCase,
    fixed: "str/strstr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strstr_oracle(),
    subject: |c| c.strstr_subject(),
}

diff_case! {
    name: strcasestr,
    case: StrCase,
    fixed: "str/strcasestr/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strcasestr_oracle(),
    subject: |c| c.strcasestr_subject(),
}

diff_case! {
    name: timingsafe_bcmp,
    case: StrCase,
    fixed: "str/timingsafe_bcmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.timingsafe_bcmp_oracle(),
    subject: |c| c.timingsafe_bcmp_subject(),
}

diff_case! {
    name: timingsafe_memcmp,
    case: StrCase,
    fixed: "str/timingsafe_memcmp/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.timingsafe_memcmp_oracle(),
    subject: |c| c.timingsafe_memcmp_subject(),
}

diff_case! {
    name: strmode,
    case: StrCase,
    fixed: "str/strmode/basic.toml",
    random: StrCase::random,
    exhaustive: (small, StrCase::exhaustive),
    oracle: |c| c.strmode_oracle(),
    subject: |c| c.strmode_subject(),
}
