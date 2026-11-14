// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The colour decision shared by the CLI and the base tools.

/// The requested colour policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decision {
    /// Colour only on a terminal, unless `NO_COLOR`/`TERM=dumb` forbids it.
    #[default]
    Auto,
    /// An explicit `--color=always`; still suppressed by JSON.
    Always,
    /// `--color=never`.
    Never,
}

impl Decision {
    /// Parse `--color`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    /// Whether ANSI may be emitted.
    ///
    /// JSON is always plain. `Always` overrides `NO_COLOR` and `TERM=dumb` for human output;
    /// `NO_COLOR` suppresses the default (`Auto`).
    #[must_use]
    pub const fn enabled(self, is_tty: bool, no_color: bool, term_dumb: bool, json: bool) -> bool {
        if json {
            return false;
        }
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => is_tty && !no_color && !term_dumb,
        }
    }
}

#[cfg(test)]
#[path = "decision_ut.rs"]
mod tests;
