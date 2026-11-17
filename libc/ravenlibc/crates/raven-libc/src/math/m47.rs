// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(exp2l, super::m43::exp2l_inner);
ld_unary!(exp10l, super::m43::exp10l_inner);
ld_unary!(expl, super::m43::expl_inner);
ld_unary!(expm1l, super::m43::expm1l_inner);
