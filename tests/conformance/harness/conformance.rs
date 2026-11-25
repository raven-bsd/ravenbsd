// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared guest-side row harness for the conformance tables.
//!
//! A topic probe includes this module, calls [`State::begin`] once, reports every row through
//! [`State::check`] and ends with [`State::finish`].  The host runner
//! (`tests/conformance/harness/runner.py`) parses exactly these lines and owns the process exit
//! mapping, so a failing row is a row result and not an ambiguous qtest exit status.
//!
//! Line contract (one serial line per record):
//!   CONFORM <topic> begin
//!   CONFORM <topic> row=<name> status=PASS|FAIL detail=<key=value ...>
//!   CONFORM <topic> summary passed=<n> total=<n> failures=<n>
//!   CONFORM <topic> done failures=<n>
//!   CONFORM-MUTATION topic=<topic> begin=<mutation>
//!   CONFORM-MUTATION topic=<topic> end=<mutation> failures=<n>

#![allow(dead_code)]

use core::fmt::{self, Write};

/// One serial line is built here; the largest row detail stays well below this.
const LINE: usize = 1024;

struct Buffer {
    data: [u8; LINE],
    len: usize,
}

impl Buffer {
    const fn new() -> Self {
        Self {
            data: [0; LINE],
            len: 0,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.data[..self.len]
    }
}

impl Write for Buffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let bytes = text.as_bytes();
        if self.len + bytes.len() > self.data.len() {
            return Err(fmt::Error);
        }
        self.data[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

unsafe extern "C" {
    #[link_name = "write"]
    fn c_write(fd: i32, buffer: *const core::ffi::c_void, count: usize) -> isize;
}

fn write_all(mut data: &[u8]) {
    while !data.is_empty() {
        // SAFETY: `data` points to `data.len()` readable bytes for the duration of the call.
        let count = unsafe { c_write(1, data.as_ptr().cast::<core::ffi::c_void>(), data.len()) };
        if count <= 0 {
            return;
        }
        data = &data[count as usize..];
    }
}

/// Write one formatted serial record.
pub fn emit(arguments: fmt::Arguments<'_>) {
    let mut buffer = Buffer::new();
    let _ = buffer.write_fmt(arguments);
    write_all(buffer.as_bytes());
}

/// Row bookkeeping for one topic.
pub struct State {
    pub topic: &'static str,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
}

impl State {
    /// Start a normal row section.
    pub fn begin(topic: &'static str) -> Self {
        emit(format_args!("CONFORM {topic} begin\n"));
        Self {
            topic,
            total: 0,
            passed: 0,
            failed: 0,
        }
    }

    /// Report one row with the exact values the row asserted.
    pub fn check(&mut self, name: &str, pass: bool, detail: fmt::Arguments<'_>) {
        self.total += 1;
        if pass {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
        let status = if pass { "PASS" } else { "FAIL" };
        emit(format_args!(
            "CONFORM {} row={} status={} detail={}\n",
            self.topic, name, status, detail
        ));
    }

    /// Print the summary and done lines; returns 0 when every row passed.
    pub fn finish(self) -> i32 {
        emit(format_args!(
            "CONFORM {} summary passed={} total={} failures={}\n",
            self.topic, self.passed, self.total, self.failed
        ));
        emit(format_args!(
            "CONFORM {} done failures={}\n",
            self.topic, self.failed
        ));
        if self.failed == 0 { 0 } else { 1 }
    }
}

/// Start a controlled-mutation section.
pub fn mutation_begin(topic: &'static str, mutation: &str) {
    emit(format_args!(
        "CONFORM-MUTATION topic={topic} begin={mutation}\n"
    ));
}

/// End a controlled-mutation section; returns 0 when no row failed.
pub fn mutation_end(state: &State, mutation: &str) -> i32 {
    emit(format_args!(
        "CONFORM-MUTATION topic={} end={} failures={}\n",
        state.topic, mutation, state.failed
    ));
    if state.failed == 0 { 0 } else { 1 }
}

/// Diagnostic line for a child or a setup detail that is not a row.
pub fn note(state: &State, detail: fmt::Arguments<'_>) {
    emit(format_args!("CONFORM {} note={}\n", state.topic, detail));
}
