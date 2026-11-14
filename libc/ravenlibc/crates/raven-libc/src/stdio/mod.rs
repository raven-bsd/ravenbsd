// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<stdio.h>` exports (`02-api-surface` I16 I25 P31 X20 X21 X23 X24 X26 L12 L16 Z15).
//!
//! The variadic entry symbols are C59 trampolines; the `v*` bodies are the FILE glue of C56 and
//! the engine of C43. `stdin`/`stdout`/`stderr` are pointers to the `.bss` objects of the C56
//! stdio state (R-19), not objects of this crate.

pub(crate) mod file;
pub(crate) mod streams;
pub(crate) mod trampolines;
pub(crate) mod trampolines_scanf;
pub(crate) mod va;
pub(crate) mod vfprintf;
pub(crate) mod vprintf;
pub(crate) mod vsnprintf;
pub(crate) mod vsprintf;
pub(crate) mod vsscanf;
