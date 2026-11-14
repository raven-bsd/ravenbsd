// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The variadic `openat(2)` entry (C59 trampoline).
crate::raven_variadic!(
    prefix: "",
    export: "openat",
    target: crate::platform::__raven_va_openat::__raven_va_openat,
    named: 3
);
