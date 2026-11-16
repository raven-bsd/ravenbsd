// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_unary!(roundl, super::m43::roundl_inner);
ld_unary!(significandl, super::m43::significandl_inner);
ld_unary!(sinl, super::m43::sinl_inner);
ld_unary!(sinhl, super::m43::sinhl_inner);
