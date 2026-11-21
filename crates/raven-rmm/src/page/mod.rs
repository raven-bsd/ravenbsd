// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Page-table entries, flags, tables, descriptor encodings and the mapper.

pub use self::{encoding::*, entry::*, flags::*, mapper::*, table::*};

mod encoding;
mod entry;
mod flags;
mod mapper;
mod table;
