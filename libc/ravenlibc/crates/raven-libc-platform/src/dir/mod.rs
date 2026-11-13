// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Directory helpers that are pure libc (the `opendir` family is C46).

pub mod getcwd;

pub use getcwd::getcwd;

#[cfg(test)]
#[path = "dir_ut.rs"]
mod tests;
