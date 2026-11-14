// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Service manifests, the enable file and the start-order check.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use raven_svc_manifest::{Manifest, Plan, start_order};

use crate::error::ConfigError;

/// The parsed service manifests and the enable file they are filtered by.
#[derive(Clone, Debug)]
pub struct ServiceSet {
    /// Manifests in directory-name order, with `enable` updated from the enable file.
    pub manifests: Vec<Manifest>,
    /// The enable file used for this set.
    pub enable_file: PathBuf,
}

impl ServiceSet {
    /// Compute the deterministic start plan; a cycle or a missing dependency is an error.
    pub fn start_order(&self) -> Result<Plan, ConfigError> {
        start_order(&self.manifests).map_err(|error| {
            ConfigError::file(
                self.enable_file
                    .parent()
                    .unwrap_or_else(|| Path::new("etc/services.d")),
                format!("service start order is invalid: {error:?}"),
            )
        })
    }
}

/// Load and validate every manifest, then apply `/etc/raven/services.enable`.
pub fn load_services(root: &Path) -> Result<ServiceSet, ConfigError> {
    let manifests = load_manifests(root)?;
    let enable_file = root.join("etc/raven/services.enable");
    let manifests = apply_enable(manifests, &enable_file)?;
    let set = ServiceSet {
        manifests,
        enable_file,
    };
    let _ = set.start_order()?;
    Ok(set)
}

/// Enable or disable one service by atomically rewriting the sorted enable file.
///
/// Returns `true` when the file changed. The manifest itself is never edited.
pub fn set_enabled(root: &Path, name: &str, enabled: bool) -> Result<bool, ConfigError> {
    let manifests = load_manifests(root)?;
    if !manifests.iter().any(|manifest| manifest.name == name) {
        return Err(ConfigError::file(
            root.join("etc/services.d"),
            format!("unknown service `{name}`"),
        ));
    }
    let enable_file = root.join("etc/raven/services.enable");
    let mut names = read_enable_file(&enable_file)?.unwrap_or_else(|| {
        manifests
            .iter()
            .filter(|m| m.enable)
            .map(|m| m.name.clone())
            .collect()
    });
    for existing in &names {
        if !manifests.iter().any(|manifest| manifest.name == *existing) {
            return Err(ConfigError::file(
                &enable_file,
                format!("enable file names unknown service `{existing}`"),
            ));
        }
    }
    let changed = if enabled {
        names.insert(name.to_string())
    } else {
        names.remove(name)
    };
    if !changed {
        return Ok(false);
    }
    write_enable_file(&enable_file, &names)?;
    Ok(true)
}

fn load_manifests(root: &Path) -> Result<Vec<Manifest>, ConfigError> {
    let directory = root.join("etc/services.d");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(ConfigError::file(
                &directory,
                format!("cannot read service directory: {error}"),
            ));
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            ConfigError::file(
                &directory,
                format!("cannot read service directory: {error}"),
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
            paths.push(path);
        }
    }
    paths.sort();
    let mut manifests = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|error| {
            ConfigError::file(&path, format!("cannot read service manifest: {error}"))
        })?;
        let manifest = Manifest::parse(&text).map_err(|error| {
            ConfigError::file(&path, format!("invalid service manifest: {:?}", error.kind))
                .with_line(error.line)
        })?;
        manifests.push(manifest);
    }
    Ok(manifests)
}

fn apply_enable(
    mut manifests: Vec<Manifest>,
    enable_file: &Path,
) -> Result<Vec<Manifest>, ConfigError> {
    let Some(names) = read_enable_file(enable_file)? else {
        return Ok(manifests);
    };
    for name in &names {
        if !manifests.iter().any(|manifest| manifest.name == *name) {
            return Err(ConfigError::file(
                enable_file,
                format!("enable file names unknown service `{name}`"),
            ));
        }
    }
    for manifest in &mut manifests {
        manifest.enable = names.contains(&manifest.name);
    }
    Ok(manifests)
}

fn read_enable_file(path: &Path) -> Result<Option<BTreeSet<String>>, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ConfigError::file(
                path,
                format!("cannot read enable file: {error}"),
            ));
        }
    };
    let mut names = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !names.insert(line.to_string()) {
            return Err(
                ConfigError::file(path, format!("duplicate service `{line}`")).with_line(index + 1),
            );
        }
    }
    Ok(Some(names))
}

fn write_enable_file(path: &Path, names: &BTreeSet<String>) -> Result<(), ConfigError> {
    let parent = path
        .parent()
        .ok_or_else(|| ConfigError::file(path, "enable file has no parent directory"))?;
    fs::create_dir_all(parent)
        .map_err(|error| ConfigError::file(path, format!("cannot create directory: {error}")))?;
    let temporary = parent.join(format!(".services.enable.{}.tmp", std::process::id()));
    let mut file = fs::File::create(&temporary)
        .map_err(|error| ConfigError::file(path, format!("cannot create enable file: {error}")))?;
    for name in names {
        writeln!(file, "{name}").map_err(|error| {
            ConfigError::file(path, format!("cannot write enable file: {error}"))
        })?;
    }
    file.sync_all()
        .map_err(|error| ConfigError::file(path, format!("cannot sync enable file: {error}")))?;
    fs::rename(&temporary, path)
        .map_err(|error| ConfigError::file(path, format!("cannot replace enable file: {error}")))
}
