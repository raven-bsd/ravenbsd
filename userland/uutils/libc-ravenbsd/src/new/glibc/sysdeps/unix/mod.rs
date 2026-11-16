// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

//! Source directory: `sysdeps/unix/`
//!
//! <https://github.com/bminor/glibc/tree/master/sysdeps/unix>

#[cfg(target_os = "linux")]
pub(crate) mod linux;
