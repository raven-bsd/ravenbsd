// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host behaviour evidence for the P4z `echo`, `date` and `man` cases.
//!
//! The target binaries need the Raven kernel, so the cases exercise the same pure argument and
//! formatting code that the `no_std` wrappers call.

#![forbid(unsafe_code)]
