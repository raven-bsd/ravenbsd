// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(cosl, super::m43::cosl_inner);
ld_unary!(coshl, super::m43::coshl_inner);
ld_unary!(erfl, super::m43::erfl_inner);
ld_unary!(erfcl, super::m43::erfcl_inner);
