// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

//! FreeBSD libc.
//!
//! * Headers: <https://github.com/freebsd/freebsd-src/blob/main/sys/riscv/include/ucontext.h>
//! * Symbol map: <https://github.com/freebsd/freebsd-src/blob/main/lib/libc/gen/Symbol.map>

pub(crate) mod sys;
pub(crate) mod unistd;
