// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

//! Emscripten libc.
//!
//! * Headers: <https://github.com/emscripten-core/emscripten/tree/main/system/lib/libc>

pub(crate) mod pthread;
pub(crate) mod sched;
pub(crate) mod unistd;
