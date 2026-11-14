// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

crate::raven_variadic!(prefix: "", export: "scanf", target: crate::stdio::vsscanf::vscanf, named: 1);
crate::raven_variadic!(prefix: "", export: "fscanf", target: crate::stdio::vsscanf::vfscanf, named: 2);
crate::raven_variadic!(prefix: "", export: "sscanf", target: crate::stdio::vsscanf::vsscanf, named: 3);
