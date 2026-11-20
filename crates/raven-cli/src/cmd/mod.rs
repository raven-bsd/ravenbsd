// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The K78 administration commands.

pub(crate) mod config;
pub(crate) mod doctor;
pub(crate) mod net;
pub(crate) mod status;
pub(crate) mod svc;

/// A backend error with a stable code and a human message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BackendError {
    pub(crate) code: String,
    pub(crate) message: String,
}

impl BackendError {
    /// Build an error with a stable code.
    pub(crate) fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    /// The common "not wired in this host binary" error.
    pub(crate) fn unavailable(what: &str) -> Self {
        Self::new(
            "EUNAVAILABLE",
            &format!("{what} backend is not wired in this host binary"),
        )
    }
}
