// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The variadic `open(3)` entry (C59 trampoline; the mode is read only when `O_CREAT`/
//! `O_TMPFILE` is set).
crate::raven_variadic!(
    prefix: "",
    export: "open",
    target: crate::platform::__raven_va_open::__raven_va_open,
    named: 2
);
