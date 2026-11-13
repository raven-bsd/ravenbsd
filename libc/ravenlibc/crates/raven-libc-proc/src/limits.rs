// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic bounds shared by the process models.

/// Maximum argument and environment bytes for one image.
pub const ARG_MAX: usize = 131_072;
/// Maximum number of argument pointers.
pub const MAX_ARGS: usize = 32_768;
/// Maximum path length, including the terminating NUL in C.
pub const PATH_MAX: usize = 4_096;
/// Largest accepted file descriptor number.
pub const MAX_FD: usize = 65_536;
/// Maximum number of atfork handlers.
pub const MAX_ATFORK: usize = 4_096;
/// Maximum number of spawn file actions.
pub const MAX_FILE_ACTIONS: usize = 4_096;
/// Maximum environment words the stack parser scans.
pub const MAX_ENV_WORDS: usize = 1 << 20;
/// Maximum auxiliary-vector pairs the stack parser accepts.
pub const MAX_AUXV_PAIRS: usize = 512;
/// Maximum supplementary group count.
pub const NGROUPS_MAX: usize = 1_023;
/// Maximum pledge string length.
pub const MAX_PLEDGE_STRING: usize = 4_096;
/// Maximum unveil table entries.
pub const MAX_UNVEIL: usize = 128;
