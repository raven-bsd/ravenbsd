// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The C-variadic `execlp(3)` entry (C59 trampoline).
crate::raven_variadic!(
    prefix: "",
    export: "execlp",
    target: crate::process::__raven_va_execlp::__raven_va_execlp,
    named: 2
);
