// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<stdlib.h>` exports, exit machinery first (`02-api-surface` I17 Z02).

pub(crate) mod exit;

pub(crate) mod abs;
pub(crate) mod atoi;
pub(crate) mod div;
pub(crate) mod strtol;
pub(crate) mod strtoul;

#[allow(non_snake_case)] // the C symbol is `_Exit`
pub(crate) mod _Exit;
pub(crate) mod abort;
