// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The C-variadic `execl(3)` entry (C59 trampoline).
crate::raven_variadic!(
    prefix: "",
    export: "execl",
    target: crate::process::__raven_va_execl::__raven_va_execl,
    named: 2
);
