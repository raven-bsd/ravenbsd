// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_value_ptr!(frexpl, super::m43::frexpl_inner);
ld_value_ptr!(modfl, super::m43::modfl_inner);
ld_value_ptr!(lgammal_r, super::m43::lgammal_r_inner);
ld_remquo!(remquol, super::m43::remquol_inner);
