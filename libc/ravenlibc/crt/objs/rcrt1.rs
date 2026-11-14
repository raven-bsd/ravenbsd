// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Fallback crate root: one `rustc --emit=obj` run for the `rcrt1` start object (C61 4.4, E2).
#![no_std]
#[macro_use]
#[path = "../src/note.rs"]
mod note;
#[path = "../src/selfreloc_consts.rs"]
mod selfreloc_consts;
#[path = "../src/selfreloc.rs"]
mod selfreloc;
#[path = "../src/rcrt1.rs"]
mod rcrt1;
