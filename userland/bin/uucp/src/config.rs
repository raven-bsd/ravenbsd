// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The bounded uucp peer and spool configuration.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// The target configuration path.
pub const CONFIG_PATH: &str = "/etc/uucp/config";
/// The default spool root.
pub const DEFAULT_SPOOL: &str = "/var/spool/uucp";
/// The default local system name.
pub const DEFAULT_LOCAL_SYSTEM: &str = "localhost";

/// Why a configuration line is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A directive is unknown.
    UnknownDirective(usize),
    /// A directive is missing its value.
    MissingValue(usize),
    /// A directive has extra words.
    ExtraValue(usize),
    /// A system name is empty or contains an invalid character.
    BadSystem(usize),
    /// The spool path is not absolute.
    BadSpool(usize),
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownDirective(line) => {
                write!(formatter, "line {line}: unknown uucp directive")
            }
            Self::MissingValue(line) => write!(formatter, "line {line}: missing value"),
            Self::ExtraValue(line) => write!(formatter, "line {line}: unexpected extra value"),
            Self::BadSystem(line) => write!(formatter, "line {line}: invalid system name"),
            Self::BadSpool(line) => write!(formatter, "line {line}: spool path must be absolute"),
        }
    }
}

/// The parsed peer configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Local system name.
    pub local_system: String,
    /// Spool root.
    pub spool: String,
    /// Known remote peers. No transport is configured, so every known peer queues.
    pub peers: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            local_system: String::from(DEFAULT_LOCAL_SYSTEM),
            spool: String::from(DEFAULT_SPOOL),
            peers: Vec::new(),
        }
    }
}

impl Config {
    /// Parse a configuration file.
    ///
    /// Blank lines and `#` comments are ignored. The accepted directives are `local <name>`,
    /// `spool <absolute-path>` and `peer <name>`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] with the one-based line number for an unknown directive, a missing
    /// or extra value, an invalid system name, or a relative spool path.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let mut config = Self::default();
        for (index, raw) in text.lines().enumerate() {
            let line = index.saturating_add(1);
            let content = raw.split('#').next().unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            let mut words = content.split_whitespace();
            let directive = words.next().unwrap_or("");
            let value = words.next().ok_or(ConfigError::MissingValue(line))?;
            if words.next().is_some() {
                return Err(ConfigError::ExtraValue(line));
            }
            match directive {
                "local" => {
                    validate_system(value, line)?;
                    config.local_system = value.to_string();
                }
                "spool" => {
                    if !value.starts_with('/') || value.contains(':') {
                        return Err(ConfigError::BadSpool(line));
                    }
                    config.spool = value.to_string();
                }
                "peer" => {
                    validate_system(value, line)?;
                    if !config.peers.iter().any(|peer| peer == value) {
                        config.peers.push(value.to_string());
                    }
                }
                _ => return Err(ConfigError::UnknownDirective(line)),
            }
        }
        Ok(config)
    }

    /// Whether a peer is configured.
    #[must_use]
    pub fn peer_known(&self, system: &str) -> bool {
        self.peers.iter().any(|peer| peer == system)
    }

    /// Join one child path below the spool root.
    #[must_use]
    pub fn path(&self, child: &str) -> String {
        if self.spool == "/" {
            format!("/{child}")
        } else {
            format!("{}/{}", self.spool.trim_end_matches('/'), child)
        }
    }

    /// The queue directory.
    #[must_use]
    pub fn queue_dir(&self) -> String {
        self.path("queue")
    }

    /// The sequence file.
    #[must_use]
    pub fn sequence_path(&self) -> String {
        self.path("seq")
    }

    /// One job file.
    #[must_use]
    pub fn job_path(&self, id: u64) -> String {
        self.path(&format!("queue/{id}.job"))
    }
}

fn validate_system(value: &str, line: usize) -> Result<(), ConfigError> {
    if value.is_empty()
        || value.contains('!')
        || value.contains('/')
        || value.chars().any(char::is_whitespace)
    {
        return Err(ConfigError::BadSystem(line));
    }
    Ok(())
}
