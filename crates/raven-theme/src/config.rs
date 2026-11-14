// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Optional std-only TOML configuration.
//!
//! The file shape follows `architecture/19-modern-tools-and-theme.md` section 3.1:
//!
//! ```toml
//! [theme]
//! name = "raven-violet"
//! variant = "dark"
//!
//! [colors]
//! accent = "#b388ff"
//! ```
//!
//! Unknown keys are errors. The user file wins over the system file. Colours are validated (hex
//! and contrast) before they are accepted. `accent_alt` is an alias for `keyword` and `foreground`
//! an alias for `path`; an explicit `keyword`/`path` in the same file wins.

use alloc::format;
use alloc::string::String;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::palette::{Palette, Rgb, Role};

/// The `[theme]` section.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSection {
    /// Palette name.
    #[serde(default)]
    pub name: Option<String>,
    /// Palette variant; only `dark` is implemented in this version.
    #[serde(default)]
    pub variant: Option<String>,
    /// Background hex, used for contrast validation.
    #[serde(default)]
    pub background: Option<String>,
}

/// The TOML file.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    /// The `[theme]` section.
    #[serde(default)]
    pub theme: ThemeSection,
    /// The `[colors]` role-to-hex map.
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
}

impl ThemeFile {
    /// Parse and validate one file.
    ///
    /// # Errors
    /// A TOML, variant, hex, unknown-colour or contrast error.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        file.validate()?;
        Ok(file)
    }

    /// Validate the optional fields.
    ///
    /// # Errors
    /// A variant, hex, unknown-colour or contrast error.
    pub fn validate(&self) -> Result<(), String> {
        let base = self.selected()?;
        for (name, text) in &self.colors {
            let role = role_for_color(name).ok_or_else(|| format!("unknown colour `{name}`"))?;
            let colour = Rgb::parse_hex(text).ok_or_else(|| format!("invalid hex `{text}`"))?;
            // Borders are non-text decoration; the other roles carry information, so they need
            // enough contrast to be readable on the selected background.
            if role != Role::Border && colour.contrast(base.background) < 3.0 {
                return Err(format!("contrast too low for `{name}`"));
            }
        }
        Ok(())
    }

    /// Build a palette from the selected built-in and the colour overrides.
    ///
    /// # Errors
    /// A validation error.
    pub fn palette(&self) -> Result<Palette, String> {
        self.validate()?;
        let mut palette = self.selected()?;
        if let Some(name) = &self.theme.name {
            palette.name = name.clone();
        }
        for (name, text) in &self.colors {
            let role = role_for_color(name).ok_or_else(|| format!("unknown colour `{name}`"))?;
            let colour = Rgb::parse_hex(text).ok_or_else(|| format!("invalid hex `{text}`"))?;
            palette.set(role, colour);
        }
        Ok(palette)
    }

    /// The built-in selected by `name`, with the optional background applied.
    fn selected(&self) -> Result<Palette, String> {
        if let Some(variant) = &self.theme.variant
            && variant != "dark"
        {
            return Err(format!("unsupported variant `{variant}`"));
        }
        let mut palette = self
            .theme
            .name
            .as_deref()
            .and_then(Palette::builtin)
            .unwrap_or_else(Palette::raven_violet);
        if let Some(background) = &self.theme.background {
            palette.background =
                Rgb::parse_hex(background).ok_or_else(|| format!("invalid hex `{background}`"))?;
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

fn role_for_color(name: &str) -> Option<Role> {
    match name {
        "accent_alt" => Some(Role::Keyword),
        "foreground" => Some(Role::Path),
        _ => Role::parse(name),
    }
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
