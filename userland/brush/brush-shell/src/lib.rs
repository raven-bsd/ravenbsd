// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Create for brush, an executable bash-compatible shell.

#![allow(dead_code)]

mod args;
mod brushctl;
pub mod bundled;
pub mod config;
pub mod entry;
mod error_formatter;
mod events;
mod productinfo;
