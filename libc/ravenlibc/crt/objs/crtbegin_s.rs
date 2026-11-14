// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Fallback crate root: one `rustc --emit=obj` run for the `crtbegin_s` start object (C61 4.4, E2).
#![no_std]
#[macro_use]
#[path = "../src/note.rs"]
mod note;
#[path = "../src/crtbegin_s.rs"]
mod crtbegin_s;
