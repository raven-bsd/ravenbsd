// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sysconf`, `pathconf` and `confstr` (COPY rows 2, 3 and 11).

pub mod confstr;
pub mod pathconf;
pub mod sysconf;

pub use confstr::confstr;
pub use pathconf::{fpathconf_value, pathconf_value};
pub use sysconf::{SysconfInputs, sysconf};

#[cfg(test)]
#[path = "conf_ut.rs"]
mod tests;
