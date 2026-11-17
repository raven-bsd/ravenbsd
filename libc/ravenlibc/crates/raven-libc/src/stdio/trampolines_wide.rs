// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The wide `printf` trampolines.

crate::raven_variadic!(prefix: "", export: "wprintf", target: crate::stdio::vwprintf::vwprintf, named: 1);
crate::raven_variadic!(prefix: "", export: "swprintf", target: crate::stdio::vwprintf::vswprintf, named: 3);
crate::raven_variadic!(prefix: "", export: "fwprintf", target: crate::stdio::vfwprintf::vfwprintf, named: 2);
