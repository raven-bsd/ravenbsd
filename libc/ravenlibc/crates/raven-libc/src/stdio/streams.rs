// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one stdio state, its three standard streams and the `.bss` buffers (R-19).

use raven_libc_stdio::{State, StdStreams};

use crate::ffi::env::ENV;

static STATE: State<crate::ffi::env::ShellEnv> = State::new(ENV);
static STREAMS: StdStreams<crate::ffi::env::ShellEnv> = StdStreams::new(&STATE);

/// The process-wide standard streams.
pub(crate) fn streams() -> &'static StdStreams<crate::ffi::env::ShellEnv> {
    &STREAMS
}
