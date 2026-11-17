// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(truncl, super::m43::truncl_inner);
ld_binary!(atan2l, super::m43::atan2l_inner);
ld_binary!(copysignl, super::m43::copysignl_inner);
ld_binary!(fdiml, super::m43::fdiml_inner);
