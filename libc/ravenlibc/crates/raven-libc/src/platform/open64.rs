// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `open64(3)`: the 64-bit `off_t` spelling of `open` (RD: `off64_t` is `off_t`).
crate::raven_variadic!(
    prefix: "",
    export: "open64",
    target: crate::platform::__raven_va_open::__raven_va_open,
    named: 2
);
