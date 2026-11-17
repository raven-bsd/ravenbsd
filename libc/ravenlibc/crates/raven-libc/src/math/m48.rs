// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(fabsl, super::m43::fabsl_inner);
ld_unary!(floorl, super::m43::floorl_inner);
ld_unary!(gammal, super::m43::gammal_inner);
ld_unary!(lgammal, super::m43::lgammal_inner);
