// SPDX-License-Identifier: MIT
// Copyright (c) 2017 Jeremy Soller
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Page-table entries, flags, tables, the descriptor encodings and the mapper.

pub use self::{encoding::*, entry::*, flags::*, mapper::*, table::*};

mod encoding;
mod entry;
mod flags;
mod mapper;
mod table;
