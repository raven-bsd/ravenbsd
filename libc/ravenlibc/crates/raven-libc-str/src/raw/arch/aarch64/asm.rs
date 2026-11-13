// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The assembled Arm routines (`__raven_str_*`).
//!
//! The C-ABI shell aliases these symbols to the C names (RD-new 1); this crate does not export
//! `extern "C"` names (L-2). The files were preprocessed once from the Arm optimized routines;
//! see each file header for the donor and the MIT election.

global_asm!(include_str!("memcpy.s"));
global_asm!(include_str!("memset.s"));
global_asm!(include_str!("memcmp.s"));
global_asm!(include_str!("strlen.s"));
global_asm!(include_str!("strnlen.s"));
global_asm!(include_str!("strchr.s"));
global_asm!(include_str!("strchrnul.s"));
global_asm!(include_str!("strrchr.s"));
global_asm!(include_str!("memchr.s"));
global_asm!(include_str!("memrchr.s"));
global_asm!(include_str!("strcmp.s"));
global_asm!(include_str!("strncmp.s"));
global_asm!(include_str!("strcpy.s"));
global_asm!(include_str!("stpcpy.s"));

// `bcmp` shares the `memcmp` body (ticket 5.7: memcmp.S also gives the bcmp entry point).
global_asm!(
    ".global __raven_str_bcmp",
    ".type __raven_str_bcmp,%function",
    ".set __raven_str_bcmp, __raven_str_memcmp"
);
