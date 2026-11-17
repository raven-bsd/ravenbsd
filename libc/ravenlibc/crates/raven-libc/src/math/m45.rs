// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(atanl, super::m43::atanl_inner);
ld_unary!(atanhl, super::m43::atanhl_inner);
ld_unary!(cbrtl, super::m43::cbrtl_inner);
ld_unary!(ceill, super::m43::ceill_inner);
