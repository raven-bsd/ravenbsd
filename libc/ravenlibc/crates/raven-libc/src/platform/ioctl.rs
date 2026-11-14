// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The variadic `ioctl(2)` entry (C59 trampoline; the third argument is one integer/pointer
//! slot and C55's dispatch decides its meaning).
crate::raven_variadic!(
    prefix: "",
    export: "ioctl",
    target: crate::platform::__raven_va_ioctl::__raven_va_ioctl,
    named: 2
);
