// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Environment variable retrieval for Unix platforms.

/// Retrieves environment variables from the host process.
///
/// On Unix, this is a direct passthrough to [`std::env::vars()`].
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    std::env::vars()
}
