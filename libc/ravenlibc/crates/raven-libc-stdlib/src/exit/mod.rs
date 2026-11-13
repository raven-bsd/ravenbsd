// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Process exit: the `atexit` table, the per-thread list, the ordering model, `abort` and
//! `quick_exit`.

pub mod abort;
pub mod atexit;
pub mod quick_exit;
pub mod sequence;
pub mod thread_atexit;

pub use abort::{AbortOps, abort_sequence};
pub use atexit::{Entry, ExitTable, Kind};
pub use quick_exit::{QuickExitTable, quick_exit_steps};
pub use sequence::exit_steps;
pub use thread_atexit::{DsoRefs, ThreadAtexit};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
