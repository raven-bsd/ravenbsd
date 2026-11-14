// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `openat64(3)`: the 64-bit `off_t` spelling of `openat`.
crate::raven_variadic!(
    prefix: "",
    export: "openat64",
    target: crate::platform::__raven_va_openat::__raven_va_openat,
    named: 3
);
