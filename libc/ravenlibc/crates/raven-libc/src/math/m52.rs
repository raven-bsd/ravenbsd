// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(sqrtl, super::m43::sqrtl_inner);
ld_unary!(tanl, super::m43::tanl_inner);
ld_unary!(tanhl, super::m43::tanhl_inner);
ld_unary!(tgammal, super::m43::tgammal_inner);
