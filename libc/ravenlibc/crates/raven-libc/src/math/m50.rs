// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(logl, super::m43::logl_inner);
ld_unary!(nearbyintl, super::m43::nearbyintl_inner);
ld_unary!(pow10l, super::m43::pow10l_inner);
ld_unary!(rintl, super::m43::rintl_inner);
