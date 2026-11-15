// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

use crate::prelude::*;

pub(crate) const _ALIGNBYTES: usize = size_of::<c_double>() - 1;

pub const _MAX_PAGE_SHIFT: u32 = 12;
