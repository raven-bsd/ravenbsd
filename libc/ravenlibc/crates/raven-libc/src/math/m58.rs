// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_sincos!(sincosl, super::m43::sincosl_inner);
ld_to_i32!(ilogbl, super::m43::ilogbl_inner);
ld_to_i64!(lrintl, super::m43::lrintl_inner);
ld_to_i64!(llrintl, super::m43::llrintl_inner);
