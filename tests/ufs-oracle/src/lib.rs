// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential UFS tests against an independent tree model.
//!
//! The model shares no code with `ufs-core`: it keeps its own path and inode
//! tables, derives link counts from references and updates contents explicitly.
//! Images are loaded from the golden vectors, operated through the public
//! `ufs-core` API and compared as a whole subtree every 250 operations.

#![forbid(unsafe_code)]

mod compare;
mod image;
mod model;
mod oracle;
mod runner;

pub use runner::{Config, SeedReport, Summary, run};
