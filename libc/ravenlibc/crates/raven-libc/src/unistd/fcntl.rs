// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The variadic `fcntl(2)` entry (C59 trampoline).
crate::raven_variadic!(
    prefix: "",
    export: "fcntl",
    target: super::__raven_va_fcntl::__raven_va_fcntl,
    named: 2
);
