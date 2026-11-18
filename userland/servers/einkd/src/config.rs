// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Configuration for the E-Ink service.
//!
//! The host core validates only what it can prove: the profile is the supported UC8179 class,
//! the interval and retry are non-zero, the backoff ceiling is not below the first retry and the
//! content source is present. The real source (HTTPS/TLS) and the panel/GPIO paths are E3/E4
//! integration inputs.

use raven_eink::Rotation;

use crate::Error;

/// Supported panel profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelProfile {
    /// Waveshare 7.5-inch V2-class UC8179, 800x480, full refresh.
    #[default]
    Waveshare75V2,
}

/// Service configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Physical panel profile.
    pub profile: PanelProfile,
    /// Logical rotation.
    pub rotation: Rotation,
    /// Scheduled refresh interval in seconds.
    pub interval_secs: u64,
    /// First retry delay after a fetch or panel failure.
    pub retry_secs: u64,
    /// Maximum exponential retry delay.
    pub max_backoff_secs: u64,
    /// Content source name; the E3 fetch layer interprets it.
    pub source: &'static str,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: PanelProfile::Waveshare75V2,
            rotation: Rotation::None,
            interval_secs: 300,
            retry_secs: 5,
            max_backoff_secs: 300,
            source: "eink",
        }
    }
}

impl Config {
    /// Validate the configuration before a service is constructed.
    pub fn validate(&self) -> Result<(), Error> {
        if self.interval_secs == 0
            || self.retry_secs == 0
            || self.max_backoff_secs < self.retry_secs
            || self.source.is_empty()
        {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
