// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Anonymous pipes backed by the operating system.

pub(crate) use std::io::{PipeReader, PipeWriter, pipe};
