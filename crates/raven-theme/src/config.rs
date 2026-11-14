// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Optional std-only TOML configuration.
//!
//! The file shape is intentionally small and strict: unknown keys are errors. The user file
//! wins over the system file. A palette is validated (hex and WCAG contrast) before it is
//! accepted.

use alloc::format;
use alloc::string::String;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::palette::{Palette, Rgb, Role};

/// The TOML file.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    /// Palette name.
    #[serde(default)]
    pub name: Option<String>,
    /// Background hex, used for contrast validation.
    #[serde(default)]
    pub background: Option<String>,
    /// Role to hex map.
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
}

impl ThemeFile {
    /// Parse and validate one file.
    ///
    /// # Errors
    /// A TOML, hex, unknown-role or contrast error.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        file.validate()?;
        Ok(file)
    }

    /// Validate the optional fields.
    ///
    /// # Errors
    /// A hex, unknown-role or contrast error.
    pub fn validate(&self) -> Result<(), String> {
        let base = self
            .name
            .as_deref()
            .and_then(Palette::builtin)
            .unwrap_or_else(Palette::raven_violet);
        let background = match &self.background {
            Some(text) => Rgb::parse_hex(text).ok_or_else(|| format!("invalid hex `{text}`"))?,
            None => base.background,
        };
        for (name, text) in &self.roles {
            let _role = Role::parse(name).ok_or_else(|| format!("unknown role `{name}`"))?;
            let colour = Rgb::parse_hex(text).ok_or_else(|| format!("invalid hex `{text}`"))?;
            if colour.contrast(background) < 4.5 {
                return Err(format!("contrast too low for `{name}`"));
            }
        }
        Ok(())
    }

    /// Build a palette, starting from Raven Violet and applying the overrides.
    ///
    /// # Errors
    /// A validation error.
    pub fn palette(&self) -> Result<Palette, String> {
        self.validate()?;
        let mut palette = self
            .name
            .as_deref()
            .and_then(Palette::builtin)
            .unwrap_or_else(Palette::raven_violet);
        if let Some(name) = &self.name {
            palette.name = name.clone();
        }
        if let Some(background) = &self.background {
            palette.background =
                Rgb::parse_hex(background).ok_or_else(|| format!("invalid hex `{background}`"))?;
        }
        for (name, text) in &self.roles {
            let role = Role::parse(name).ok_or_else(|| format!("unknown role `{name}`"))?;
            let colour = Rgb::parse_hex(text).ok_or_else(|| format!("invalid hex `{text}`"))?;
            palette.set(role, colour);
        }
        Ok(palette)
    }

    /// The system path.
    #[must_use]
    pub fn system_path() -> PathBuf {
        PathBuf::from("/etc/raven/theme.toml")
    }

    /// The user path, honouring `XDG_CONFIG_HOME`.
    #[must_use]
    pub fn user_path() -> PathBuf {
        if let Some(home) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(home).join("raven/theme.toml");
        }
        let home = std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/raven/theme.toml")
    }

    /// Load the user file if present, else the system file.
    ///
    /// # Errors
    /// A read or validation error.
    pub fn load() -> Result<Palette, String> {
        Self::load_from(&Self::user_path(), &Self::system_path())
    }

    /// Load the first existing file, user before system.
    ///
    /// Paths are explicit so tests never touch the real home directory.
    ///
    /// # Errors
    /// A read or validation error.
    pub fn load_from(user: &Path, system: &Path) -> Result<Palette, String> {
        for path in [user, system] {
            if path.is_file() {
                return Self::from_path(path);
            }
        }
        Ok(Palette::raven_violet())
    }

    /// Load one file.
    ///
    /// # Errors
    /// A read or validation error.
    pub fn from_path(path: &Path) -> Result<Palette, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::parse(&text)?.palette()
    }
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
