// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<stdlib.h>`, `<search.h>` and `<getopt.h>` exports, exit machinery first
//! (`02-api-surface` I17 Z02, C60c).

pub(crate) mod exit;
// The exit registry's file is `stdlib/atexit.rs`, but the module is declared from `lib.rs` so
// its host unit tests build although `stdlib` itself is target-only (C61 integration).

pub(crate) mod a64l;
pub(crate) mod abs;
pub(crate) mod atoi;
pub(crate) mod bsearch;
pub(crate) mod clearenv;
pub(crate) mod div;
pub(crate) mod drand48;
pub(crate) mod env_support;
pub(crate) mod environ;
pub(crate) mod erand48;
pub(crate) mod getenv;
pub(crate) mod getenv_r;
pub(crate) mod getopt;
pub(crate) mod getopt_long;
pub(crate) mod getopt_long_only;
pub(crate) mod getopt_support;
pub(crate) mod getsubopt;
pub(crate) mod heapsort;
pub(crate) mod initstate;
pub(crate) mod insque;
pub(crate) mod jrand48;
pub(crate) mod l64a;
pub(crate) mod lcong48;
pub(crate) mod lfind;
pub(crate) mod lrand48;
pub(crate) mod lsearch;
pub(crate) mod mergesort;
pub(crate) mod mrand48;
pub(crate) mod nrand48;
pub(crate) mod optarg;
pub(crate) mod opterr;
pub(crate) mod optind;
pub(crate) mod optopt;
pub(crate) mod optreset;
pub(crate) mod putenv;
pub(crate) mod qsort;
pub(crate) mod qsort_r;
pub(crate) mod rand;
pub(crate) mod rand_r;
pub(crate) mod rand_state;
pub(crate) mod random;
pub(crate) mod remque;
pub(crate) mod search_support;
pub(crate) mod secure_getenv;
pub(crate) mod seed48;
pub(crate) mod setenv;
pub(crate) mod setstate;
pub(crate) mod sort_support;
pub(crate) mod srand;
pub(crate) mod srand48;
pub(crate) mod srandom;
pub(crate) mod strtol;
pub(crate) mod strtonum;
pub(crate) mod strtoul;
pub(crate) mod suboptarg;
pub(crate) mod system;
pub(crate) mod tdelete;
pub(crate) mod tdestroy;
pub(crate) mod tfind;
pub(crate) mod tsearch;
pub(crate) mod twalk;
pub(crate) mod unsetenv;

#[allow(non_snake_case)] // the C symbol is `_Exit`
pub(crate) mod _Exit;
pub(crate) mod abort;
