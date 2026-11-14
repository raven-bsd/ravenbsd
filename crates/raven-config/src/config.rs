// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The loader, precedence and typed accessors.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ConfigError;
use crate::merge::merge;
use crate::node::Node;
use crate::schema::validate;

/// A loaded, validated effective configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    node: Node,
    profile: String,
    sources: Vec<PathBuf>,
}

impl Config {
    /// The effective configuration tree.
    #[must_use]
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// The selected profile name.
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// The number of files that contributed to the result.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// The files that contributed, in load order.
    #[must_use]
    pub fn sources(&self) -> &[PathBuf] {
        &self.sources
    }

    /// Return the value at a dotted key.
    #[must_use]
    pub fn get(&self, dotted: &str) -> Option<&Node> {
        self.node.get(dotted)
    }

    /// Render the effective configuration as TOML.
    pub fn to_toml_string(&self) -> Result<String, ConfigError> {
        let value = self
            .node
            .to_toml()
            .ok_or_else(|| ConfigError::file("", "the configuration contains a null root"))?;
        toml::to_string_pretty(&value)
            .map_err(|error| ConfigError::file("", format!("cannot render TOML: {error}")))
    }

    /// Render the effective configuration as compact JSON.
    pub fn to_json_string(&self) -> Result<String, ConfigError> {
        serde_json::to_string(&self.node.to_json())
            .map_err(|error| ConfigError::file("", format!("cannot render JSON: {error}")))
    }
}

/// Load the fixed configuration directories below one root.
#[derive(Clone, Debug)]
pub struct ConfigLoader {
    root: PathBuf,
    profile: Option<String>,
}

impl ConfigLoader {
    /// Build a loader for `root` (use `/` on the target).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            profile: None,
        }
    }

    /// Force one profile name, ignoring the `profile` key of `raven.toml`.
    #[must_use]
    pub fn with_profile(mut self, profile: Option<&str>) -> Self {
        self.profile = profile.map(String::from);
        self
    }

    /// Load and validate the effective configuration.
    pub fn load(&self) -> Result<Config, ConfigError> {
        let base = self.root.join("etc/raven");
        let main_path = base.join("raven.toml");
        let main_node = load_optional(&main_path)?;
        let explicit = self.profile.is_some();
        let profile = match &self.profile {
            Some(profile) => profile.clone(),
            None => main_node
                .as_ref()
                .and_then(|node| node.get("profile"))
                .and_then(Node::as_str)
                .unwrap_or("base")
                .to_string(),
        };
        if profile.is_empty() {
            return Err(ConfigError::file(&main_path, "profile name is empty"));
        }

        let mut sources = Vec::new();
        let mut effective = defaults();
        let profile_path = base.join("profiles").join(format!("{profile}.toml"));
        if let Some(node) = load_optional(&profile_path)? {
            validate(&profile_path, &node)?;
            effective = merge(&effective, &node);
            sources.push(profile_path);
        } else if explicit || main_node.is_some() {
            return Err(ConfigError::file(
                &profile_path,
                format!("selected profile `{profile}` is missing"),
            ));
        }

        if let Some(node) = main_node {
            validate(&main_path, &node)?;
            effective = merge(&effective, &node);
            sources.push(main_path);
        }

        let drop_in_dir = base.join("conf.d");
        for path in sorted_toml_files(&drop_in_dir)? {
            let node = load_required(&path)?;
            validate(&path, &node)?;
            effective = merge(&effective, &node);
            sources.push(path);
        }

        Ok(Config {
            node: effective,
            profile,
            sources,
        })
    }
}

/// The compiled defaults: a machine boots with no `/etc/raven/raven.toml`.
#[must_use]
pub fn defaults() -> Node {
    let mut status = BTreeMap::new();
    status.insert(
        "core_services".to_string(),
        Node::Array(vec![
            Node::String("logd".to_string()),
            Node::String("consd".to_string()),
            Node::String("vfsd".to_string()),
        ]),
    );
    let mut log = BTreeMap::new();
    log.insert("level".to_string(), Node::String("info".to_string()));
    let mut services = BTreeMap::new();
    services.insert(
        "enable_file".to_string(),
        Node::String(crate::DEFAULT_ENABLE_FILE.to_string()),
    );
    let mut table = BTreeMap::new();
    table.insert("profile".to_string(), Node::String("base".to_string()));
    table.insert("color".to_string(), Node::String("auto".to_string()));
    table.insert("log".to_string(), Node::Table(log));
    table.insert("services".to_string(), Node::Table(services));
    table.insert("status".to_string(), Node::Table(status));
    Node::Table(table)
}

fn load_optional(path: &Path) -> Result<Option<Node>, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => parse(path, &text).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ConfigError::file(
            path,
            format!("cannot read configuration: {error}"),
        )),
    }
}

fn load_required(path: &Path) -> Result<Node, ConfigError> {
    let text = fs::read_to_string(path)
        .map_err(|error| ConfigError::file(path, format!("cannot read configuration: {error}")))?;
    parse(path, &text)
}

fn parse(path: &Path, text: &str) -> Result<Node, ConfigError> {
    let value: toml::Value = toml::from_str(text).map_err(|error| {
        let line = error.span().map(|span| {
            text[..span.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1
        });
        let mut config_error = ConfigError::file(path, error.to_string());
        if let Some(line) = line {
            config_error = config_error.with_line(line);
        }
        config_error
    })?;
    Ok(Node::from_toml(value))
}

fn sorted_toml_files(directory: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(ConfigError::file(
                directory,
                format!("cannot read drop-in directory: {error}"),
            ));
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            ConfigError::file(directory, format!("cannot read drop-in directory: {error}"))
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}
