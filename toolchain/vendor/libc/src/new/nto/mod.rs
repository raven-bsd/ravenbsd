// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

//! QNX Neutrino libc.
// FIXME(nto): link to manpages needed.

pub(crate) mod unistd;

pub(crate) mod net {
    pub(crate) mod bpf;
    pub(crate) mod if_;
}
