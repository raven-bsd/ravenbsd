// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The wide `scanf` trampolines.

crate::raven_variadic!(prefix: "", export: "wscanf", target: crate::stdio::vwprintf::vwscanf, named: 1);
crate::raven_variadic!(prefix: "", export: "fwscanf", target: crate::stdio::vwprintf::vfwscanf, named: 2);
crate::raven_variadic!(prefix: "", export: "swscanf", target: crate::stdio::vswscanf::vswscanf, named: 2);
