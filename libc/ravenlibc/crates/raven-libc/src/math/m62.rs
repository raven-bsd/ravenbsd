// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `nearbyint` and `nearbyintf`.

unary!(nearbyint, raven_libc_m::nearbyint);
unaryf!(nearbyintf, raven_libc_m::nearbyintf);
