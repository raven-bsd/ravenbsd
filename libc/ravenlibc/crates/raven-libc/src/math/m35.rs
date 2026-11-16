// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `remainder`, `remainderf`, `remquo` and `remquof`.

binary!(remainder, raven_libc_m::remainder);
binaryf!(remainderf, raven_libc_m::remainderf);
remquo!(remquo, raven_libc_m::remquo, core::ffi::c_double);
remquo!(remquof, raven_libc_m::remquof, core::ffi::c_float);
