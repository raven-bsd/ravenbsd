// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Layer D: the delivery decision machine (pending/mask/handler-mask) against the independent
//! model. Handler execution through the fake kernel is covered by the crate's `tests/delivery.rs`.

#![cfg(feature = "oracle-signal-musl")]
#![allow(clippy::manual_range_contains)]
use raven_libc_oracle::diff_case;
use raven_libc_oracle::support_signal::DecisionCase;

diff_case! {
    name: delivery_decisions,
    case: DecisionCase,
    fixed: "signal/delivery/basic.toml",
    random: DecisionCase::random,
    exhaustive: (small, DecisionCase::exhaustive_small),
    oracle: |c| c.run_model(),
    subject: |c| c.run_subject(),
}
