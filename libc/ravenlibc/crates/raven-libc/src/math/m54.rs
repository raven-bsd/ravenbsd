// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_binary!(fmaxl, super::m43::fmaxl_inner);
ld_binary!(fminl, super::m43::fminl_inner);
ld_binary!(fmodl, super::m43::fmodl_inner);
ld_binary!(hypotl, super::m43::hypotl_inner);
