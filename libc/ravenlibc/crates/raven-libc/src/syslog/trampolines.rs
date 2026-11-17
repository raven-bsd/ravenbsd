// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Variadic trampolines for `syslog`.

crate::raven_variadic!(
    prefix: "",
    export: "syslog",
    target: crate::syslog::vsyslog::vsyslog,
    named: 2
);
