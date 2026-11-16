// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one stdio state, its three standard streams and the `.bss` buffers (R-19).

use core::ffi::c_void;
use core::sync::atomic::Ordering;

use raven_libc_stdio::{State, StdStreams};

use crate::ffi::env::ENV;
use crate::stdio::{stderr, stdin, stdout};

static STATE: State<crate::ffi::env::ShellEnv> = State::new(ENV);
static STREAMS: StdStreams<crate::ffi::env::ShellEnv> = StdStreams::new(&STATE);

/// The process-wide standard streams.
pub(crate) fn streams() -> &'static StdStreams<crate::ffi::env::ShellEnv> {
    &STREAMS
}

/// Publish the three `FILE *` data symbols before the program's constructors run.
pub(crate) fn init() {
    let [input, output, error] = STREAMS.all();
    stdin::stdin.store(input as *const _ as *mut c_void, Ordering::Release);
    stdout::stdout.store(output as *const _ as *mut c_void, Ordering::Release);
    stderr::stderr.store(error as *const _ as *mut c_void, Ordering::Release);
}
