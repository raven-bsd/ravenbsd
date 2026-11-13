// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The C-variadic entry symbols of this ticket: `printf fprintf sprintf snprintf`.
//!
//! Each line is one C59 `global_asm!` stub (`raven_variadic!`); the target is the `v*` function
//! of the same family. On aarch64 and riscv64 the stubs are link-time failing until a qemu-user
//! lane verifies the layout (C59 README, H-16).

crate::raven_variadic!(prefix: "", export: "printf", target: crate::stdio::vprintf::vprintf, named: 1);
crate::raven_variadic!(prefix: "", export: "fprintf", target: crate::stdio::vfprintf::vfprintf, named: 2);
crate::raven_variadic!(prefix: "", export: "sprintf", target: crate::stdio::vsprintf::vsprintf, named: 2);
crate::raven_variadic!(prefix: "", export: "snprintf", target: crate::stdio::vsnprintf::vsnprintf, named: 3);
