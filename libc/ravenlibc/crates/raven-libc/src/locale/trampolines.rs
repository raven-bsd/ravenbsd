// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Variadic trampolines for `strfmon` and `strfmon_l`.

crate::raven_variadic!(
    prefix: "",
    export: "strfmon",
    target: crate::locale::monetary::__raven_va_strfmon,
    named: 3
);
crate::raven_variadic!(
    prefix: "",
    export: "strfmon_l",
    target: crate::locale::monetary::__raven_va_strfmon_l,
    named: 4
);
