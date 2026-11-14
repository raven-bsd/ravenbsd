// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fifteen target bodies (fourteen implemented; `iconv_stream` is DEFERRED on C48).
//!
//! Each module exposes `pub fn body(data: &[u8])` and a sibling `_ut.rs` with three tests:
//! a hostile clean input, the boundary inputs of the ticket and a mutation proof.

pub mod elf_dynamic;
pub mod getopt_args;
pub mod glob_pattern;
pub mod inet_text;
pub mod printf_format;
pub mod regex_compile_exec;
pub mod resolver_conf;
pub mod resolver_message;
pub mod scanf_format;
pub mod strtod_input;
pub mod strtol_input;
pub mod time_format;
pub mod tz_parse;
pub mod utf8_decode;
