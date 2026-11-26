// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The signal shell family (C64): disposition table, set operations and the send wrappers.

use raven_libc_signal::DispTable;

/// The process disposition table (C58 owns the state machine).
pub(crate) static DISP: DispTable = DispTable::new();

#[path = "kill.rs"]
mod kill;
#[path = "killpg.rs"]
mod killpg;
#[path = "psignal.rs"]
mod psignal;
#[path = "raise.rs"]
mod raise;
#[path = "sig2str.rs"]
mod sig2str;
#[path = "sigaction.rs"]
mod sigaction;
#[path = "sigaddset.rs"]
mod sigaddset;
#[path = "sigaltstack.rs"]
mod sigaltstack;
#[path = "sigandset.rs"]
mod sigandset;
#[path = "sigdelset.rs"]
mod sigdelset;
#[path = "sigemptyset.rs"]
mod sigemptyset;
#[path = "sigfillset.rs"]
mod sigfillset;
#[path = "sighold.rs"]
mod sighold;
#[path = "sigignore.rs"]
mod sigignore;
#[path = "siginterrupt.rs"]
mod siginterrupt;
#[path = "sigisemptyset.rs"]
mod sigisemptyset;
#[path = "sigismember.rs"]
mod sigismember;
#[path = "signal.rs"]
mod signal;
#[path = "sigorset.rs"]
mod sigorset;
#[path = "sigpause.rs"]
mod sigpause;
#[path = "sigpending.rs"]
mod sigpending;
#[path = "sigprocmask.rs"]
mod sigprocmask;
pub(crate) use sigprocmask::sigprocmask;
#[path = "sigqueue.rs"]
mod sigqueue;
#[path = "sigrelse.rs"]
mod sigrelse;
#[path = "sigset.rs"]
mod sigset;
#[path = "sigsuspend.rs"]
mod sigsuspend;
#[path = "sigtimedwait.rs"]
mod sigtimedwait;
#[path = "sigwait.rs"]
mod sigwait;
#[path = "sigwaitinfo.rs"]
mod sigwaitinfo;
#[path = "str2sig.rs"]
mod str2sig;
