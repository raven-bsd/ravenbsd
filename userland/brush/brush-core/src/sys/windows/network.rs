// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

pub(crate) fn get_hostname() -> std::io::Result<std::ffi::OsString> {
    crate::sys::hostname::get()
}
