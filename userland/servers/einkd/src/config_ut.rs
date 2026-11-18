// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the service configuration.

use super::*;

#[test]
fn default_config_is_valid() {
    let config = Config::default();
    assert_eq!(config.profile, PanelProfile::Waveshare75V2);
    assert!(config.validate().is_ok());
}

#[test]
fn inconsistent_config_is_rejected() {
    assert_eq!(
        Config {
            interval_secs: 0,
            ..Config::default()
        }
        .validate(),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        Config {
            retry_secs: 0,
            ..Config::default()
        }
        .validate(),
        Err(Error::InvalidConfig)
    );
    let default = Config::default();
    assert_eq!(
        Config {
            max_backoff_secs: default.retry_secs - 1,
            ..default
        }
        .validate(),
        Err(Error::InvalidConfig)
    );
    assert_eq!(
        Config {
            source: "",
            ..Config::default()
        }
        .validate(),
        Err(Error::InvalidConfig)
    );
}
