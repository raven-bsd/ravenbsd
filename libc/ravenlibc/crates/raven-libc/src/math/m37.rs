// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pown` and `rootn`.

pown!(pown, raven_libc_m::pown, core::ffi::c_double);
pown!(pownf, raven_libc_m::pownf, core::ffi::c_float);
pown!(rootn, raven_libc_m::rootn, core::ffi::c_double);
pown!(rootnf, raven_libc_m::rootnf, core::ffi::c_float);
