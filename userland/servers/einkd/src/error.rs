// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Service error type.

use raven_eink::Error as PanelError;

/// Why a service step could not complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The configuration is internally inconsistent.
    InvalidConfig,
    /// The content provider failed; the last-good frame is kept.
    Fetch,
    /// The panel/encoder failed; the last-good frame is kept.
    Panel(PanelError),
}

impl From<PanelError> for Error {
    fn from(error: PanelError) -> Self {
        Self::Panel(error)
    }
}
