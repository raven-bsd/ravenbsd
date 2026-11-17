// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_binary!(nextafterl, super::m43::nextafterl_inner);
ld_binary!(nexttowardl, super::m43::nexttowardl_inner);
ld_binary!(powl, super::m43::powl_inner);
ld_binary!(remainderl, super::m43::remainderl_inner);
