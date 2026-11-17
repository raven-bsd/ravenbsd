// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(log10l, super::m43::log10l_inner);
ld_unary!(log1pl, super::m43::log1pl_inner);
ld_unary!(log2l, super::m43::log2l_inner);
ld_unary!(logbl, super::m43::logbl_inner);
