// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Layer C: random `sigaction` sequences against the independent model of musl `sigaction.c`.

#![cfg(feature = "oracle-signal-musl")]
#![allow(clippy::manual_range_contains)]
use raven_libc_oracle::diff_case;
use raven_libc_oracle::support_signal::DispositionCase;

diff_case! {
    name: disposition_sequences,
    case: DispositionCase,
    fixed: "signal/dispositions/basic.toml",
    random: DispositionCase::random,
    exhaustive: (small, DispositionCase::exhaustive_small),
    oracle: |c| c.run_model(),
    subject: |c| c.run_subject(),
}
