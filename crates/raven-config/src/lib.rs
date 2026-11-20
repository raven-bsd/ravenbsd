// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Strict TOML configuration for the `raven` administration surface.
//!
//! The loader reads the fixed directories of `architecture/17-modern-userland.md` section 4:
//! compiled defaults, the selected profile, `/etc/raven/raven.toml`, and the sorted
//! `conf.d/*.toml` drop-ins. Tables merge recursively, scalars and arrays replace, and a [`Node`]
//! value of [`Node::Null`] removes a key (the merge API supports it even though TOML itself has
//! no null literal). Every file is validated against the small known schema before it is merged,
//! so an unknown key or a wrong type names the file and key and is never silently ignored.
//!
//! The service-enable file and the start-order check use the first-party
//! [`raven_svc_manifest`] model; no second service parser exists.

#![forbid(unsafe_code)]

mod config;
mod error;
mod merge;
mod network;
mod node;
mod schema;
mod services;
pub mod users;

pub use config::{Config, ConfigLoader};
pub use error::ConfigError;
pub use merge::merge;
pub use network::{
    HOSTNAME_MAX, NETWORK_DROP_IN, NetworkConfig, NetworkMode, StaticNetwork,
    from_node as network_from_node, validate_hostname, write_drop_in as write_network_drop_in,
};
pub use node::Node;
pub use schema::{check_secret_mode, parse_duration, parse_size, validate_path};
pub use services::{ServiceSet, load_services, set_enabled};

/// The default enable file, relative to the configuration root's `/etc`.
pub const DEFAULT_ENABLE_FILE: &str = "/etc/raven/services.enable";
