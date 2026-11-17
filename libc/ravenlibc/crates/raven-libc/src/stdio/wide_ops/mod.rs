// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The narrow `FILE` wide-character entry points over `raven-libc-stdio::wide`.

pub(crate) mod fgetws;
pub(crate) mod fputwc_unlocked;
pub(crate) mod fwide;
pub(crate) mod getwc_unlocked;
pub(crate) mod putwchar_unlocked;
