// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The signal shell family (C64): disposition table, set operations and the send wrappers.

use raven_libc_signal::DispTable;

/// The process disposition table (C58 owns the state machine).
pub(crate) static DISP: DispTable = DispTable::new();

#[path = "kill.rs"]
mod kill;
#[path = "raise.rs"]
mod raise;
#[path = "sig2str.rs"]
mod sig2str;
#[path = "sigaddset.rs"]
mod sigaddset;
#[path = "sigandset.rs"]
mod sigandset;
#[path = "sigdelset.rs"]
mod sigdelset;
#[path = "sigemptyset.rs"]
mod sigemptyset;
#[path = "sigfillset.rs"]
mod sigfillset;
#[path = "sigisemptyset.rs"]
mod sigisemptyset;
#[path = "sigismember.rs"]
mod sigismember;
#[path = "signal_fn.rs"]
mod signal;
#[path = "sigorset.rs"]
mod sigorset;
#[path = "str2sig.rs"]
mod str2sig;
