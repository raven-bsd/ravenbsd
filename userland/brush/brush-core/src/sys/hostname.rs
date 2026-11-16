// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

pub(crate) fn get() -> std::io::Result<std::ffi::OsString> {
    hostname::get()
}
