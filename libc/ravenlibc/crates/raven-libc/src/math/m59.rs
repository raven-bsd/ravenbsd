// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_to_i64!(lroundl, super::m43::lroundl_inner);
ld_to_i64!(llroundl, super::m43::llroundl_inner);
ld_to_i64!(llogbl, super::m43::llogbl_inner);
ld_nan!(nanl, super::m43::nanl_inner);
