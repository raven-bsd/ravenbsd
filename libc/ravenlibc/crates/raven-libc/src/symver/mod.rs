// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `.symver` compatibility symbols.
//!
//! `RAVEN_1.0` is the first release node and nothing older exists, so this module is empty
//! (`02-api-surface` Z12). A future node adds its `.symver` lines here; the module stays in the
//! shell so `Symbol.map` and the build never grow a second compat mechanism.
