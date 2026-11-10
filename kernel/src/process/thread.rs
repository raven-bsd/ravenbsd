// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread object re-export.
//!
//! S09 moved the thread object into `sched::thread`; `process::thread` remains the path used by
//! the process objects and handle table.

pub use crate::sched::thread::Thread;
