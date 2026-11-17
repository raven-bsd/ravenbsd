// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `nan` and `nanf`.

nan!(nan, raven_libc_m::nan, core::ffi::c_double);
nan!(nanf, raven_libc_m::nanf, core::ffi::c_float);
