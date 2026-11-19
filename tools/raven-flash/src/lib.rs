// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-flash`: safe removable-media image writer with a terminal UI.

#![forbid(unsafe_code)]

pub mod cli;
pub mod device;
pub mod image;
pub mod tui;
pub mod writer;
