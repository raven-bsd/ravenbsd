// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `powr` and `compoundn`.

binary!(powr, raven_libc_m::powr);
binaryf!(powrf, raven_libc_m::powrf);
compoundn!(compoundn, raven_libc_m::compoundn, core::ffi::c_double);
compoundn!(compoundnf, raven_libc_m::compoundnf, core::ffi::c_float);
