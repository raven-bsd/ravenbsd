// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Human-only colour policy.

/// When a human writer may use ANSI colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Color {
    /// Colour only when stdout is a terminal and `NO_COLOR` is not set.
    #[default]
    Auto,
    /// Always use colour; an explicit policy overrides `NO_COLOR`.
    Always,
    /// Never use colour.
    Never,
}

impl Color {
    /// Parse the value of `--color`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "always" => Some(Self::Always),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    /// Resolve the policy for one human writer.
    ///
    /// `Auto` follows the terminal and `NO_COLOR`; an explicit `Always` keeps colour even when
    /// `NO_COLOR` is present. JSON output never calls this policy.
    #[must_use]
    pub const fn enabled(self, is_tty: bool, no_color: bool) -> bool {
        match self {
            Self::Auto => is_tty && !no_color,
            Self::Always => true,
            Self::Never => false,
        }
    }
}
