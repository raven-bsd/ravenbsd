// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the configuration loader, merge rules and service enable state.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use raven_config::{
    ConfigLoader, Node, load_services, merge, parse_duration, parse_size, set_enabled,
};

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-config-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn write_mode(&self, relative: &str, text: &str, mode: u32) {
        self.write(relative, text);
        let path = self.path.join(relative);
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn defaults_apply_without_any_file() {
    let root = TempDir::new("defaults");
    let config = ConfigLoader::new(root.path()).load().unwrap();
    assert_eq!(config.profile(), "base");
    assert_eq!(config.source_count(), 0);
    assert_eq!(config.get("color").and_then(Node::as_str), Some("auto"));
    assert_eq!(
        config.get("services.enable_file").and_then(Node::as_str),
        Some("/etc/raven/services.enable")
    );
    let json = config.to_json_string().unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["profile"], "base");
    assert!(config.to_toml_string().unwrap().contains("profile"));
}

#[test]
fn precedence_is_profile_main_then_sorted_drop_ins() {
    let root = TempDir::new("precedence");
    root.write(
        "etc/raven/profiles/base.toml",
        "color = \"never\"\n[log]\nlevel = \"warn\"\n",
    );
    root.write(
        "etc/raven/raven.toml",
        "profile = \"base\"\ncolor = \"always\"\n[log]\nring_bytes = \"8 MiB\"\n",
    );
    root.write("etc/raven/conf.d/20-color.toml", "color = \"auto\"\n");
    root.write("etc/raven/conf.d/10-log.toml", "[log]\nlevel = \"debug\"\n");
    let config = ConfigLoader::new(root.path()).load().unwrap();
    assert_eq!(config.profile(), "base");
    assert_eq!(config.get("color").and_then(Node::as_str), Some("auto"));
    assert_eq!(
        config.get("log.level").and_then(Node::as_str),
        Some("debug")
    );
    assert_eq!(
        config.get("log.ring_bytes").and_then(Node::as_str),
        Some("8 MiB")
    );
    assert_eq!(config.source_count(), 4);
}

#[test]
fn arrays_replace_and_tables_merge() {
    let root = TempDir::new("arrays");
    root.write(
        "etc/raven/profiles/base.toml",
        "[status]\ncore_services = [\"a\"]\n[log]\nlevel = \"warn\"\n",
    );
    root.write(
        "etc/raven/raven.toml",
        "profile = \"base\"\n[status]\ncore_services = [\"b\"]\n",
    );
    root.write(
        "etc/raven/conf.d/10.toml",
        "[status]\ncore_services = [\"c\", \"d\"]\n[log]\nretention = \"2h\"\n",
    );
    let config = ConfigLoader::new(root.path()).load().unwrap();
    let services = config
        .get("status.core_services")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(services.len(), 2);
    assert_eq!(services[0].as_str(), Some("c"));
    assert_eq!(services[1].as_str(), Some("d"));
    assert_eq!(
        config.get("log.retention").and_then(Node::as_str),
        Some("2h")
    );
    assert_eq!(config.get("log.level").and_then(Node::as_str), Some("warn"));
}

#[test]
fn null_removes_a_key() {
    let mut base = BTreeMap::new();
    base.insert("a".to_string(), Node::Integer(1));
    base.insert("b".to_string(), Node::Integer(2));
    let mut overlay = BTreeMap::new();
    overlay.insert("a".to_string(), Node::Null);
    let merged = merge(&Node::Table(base), &Node::Table(overlay));
    let table = merged.as_table().unwrap();
    assert!(!table.contains_key("a"));
    assert_eq!(table.get("b").and_then(Node::as_i64), Some(2));
}

#[test]
fn unknown_keys_and_wrong_types_are_strict() {
    let root = TempDir::new("strict");
    root.write("etc/raven/profiles/base.toml", "profile = \"base\"\n");
    root.write(
        "etc/raven/raven.toml",
        "profile = \"base\"\nnot_a_key = 1\n",
    );
    let error = ConfigLoader::new(root.path()).load().unwrap_err();
    assert_eq!(error.key.as_deref(), Some("not_a_key"));
    assert!(error.file.ends_with("etc/raven/raven.toml"));

    let root = TempDir::new("wrong-type");
    root.write("etc/raven/profiles/base.toml", "color = 1\n");
    let error = ConfigLoader::new(root.path()).load().unwrap_err();
    assert_eq!(error.key.as_deref(), Some("color"));
    assert!(error.to_string().contains("must be a string"));
}

#[test]
fn parse_errors_report_a_line() {
    let root = TempDir::new("line");
    root.write("etc/raven/profiles/base.toml", "color = \"auto\"\nbroken =");
    let error = ConfigLoader::new(root.path()).load().unwrap_err();
    assert_eq!(error.line, Some(2));
}

#[test]
fn size_and_duration_forms_are_exact() {
    assert_eq!(parse_size(&Node::Integer(42)).unwrap(), 42);
    assert_eq!(
        parse_size(&Node::String("8 MiB".to_string())).unwrap(),
        8 * 1024 * 1024
    );
    assert_eq!(
        parse_size(&Node::String("1.5GiB".to_string())).unwrap(),
        3 * 512 * 1024 * 1024
    );
    assert!(parse_size(&Node::String("8 MiB extra".to_string())).is_err());
    assert_eq!(parse_duration(&Node::Integer(250)).unwrap(), 250);
    assert_eq!(
        parse_duration(&Node::String("250ms".to_string())).unwrap(),
        250
    );
    assert_eq!(
        parse_duration(&Node::String("2h".to_string())).unwrap(),
        7_200_000
    );
    assert!(parse_duration(&Node::String("2 weeks".to_string())).is_err());
}

#[test]
fn secret_files_must_not_be_world_readable() {
    let root = TempDir::new("secret");
    root.write_mode("token", "s3cret", 0o600);
    let secret = root.path().join("token");
    root.write(
        "etc/raven/profiles/base.toml",
        &format!("[secrets]\ntoken = \"{}\"\n", secret.display()),
    );
    ConfigLoader::new(root.path()).load().unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
    let error = ConfigLoader::new(root.path()).load().unwrap_err();
    assert!(error.to_string().contains("world-readable"));
}

#[test]
fn dotted_get_reaches_nested_values() {
    let root = TempDir::new("get");
    root.write("etc/raven/profiles/base.toml", "[log]\nlevel = \"info\"\n");
    let config = ConfigLoader::new(root.path()).load().unwrap();
    assert_eq!(config.get("log.level").and_then(Node::as_str), Some("info"));
    assert!(config.get("log.nope").is_none());
}

#[test]
fn service_manifests_load_and_apply_the_enable_file() {
    let root = TempDir::new("services");
    root.write("etc/services.d/a.toml", "name = \"a\"\nexec = \"/bin/a\"\n");
    root.write("etc/services.d/b.toml", "name = \"b\"\nexec = \"/bin/b\"\n");
    let set = load_services(root.path()).unwrap();
    assert_eq!(set.manifests.len(), 2);
    assert_eq!(set.start_order().unwrap().order().len(), 2);
    root.write("etc/raven/services.enable", "b\n");
    let set = load_services(root.path()).unwrap();
    let a = set.manifests.iter().find(|m| m.name == "a").unwrap();
    let b = set.manifests.iter().find(|m| m.name == "b").unwrap();
    assert!(!a.enable);
    assert!(b.enable);
}

#[test]
fn a_manifest_cycle_is_an_error() {
    let root = TempDir::new("cycle");
    root.write(
        "etc/services.d/a.toml",
        "name = \"a\"\nexec = \"/bin/a\"\nneeds = [\"b\"]\n",
    );
    root.write(
        "etc/services.d/b.toml",
        "name = \"b\"\nexec = \"/bin/b\"\nneeds = [\"a\"]\n",
    );
    let error = load_services(root.path()).unwrap_err();
    assert!(error.to_string().contains("start order"));
}

#[test]
fn enable_and_disable_are_idempotent_and_sorted() {
    let root = TempDir::new("enable");
    root.write("etc/services.d/a.toml", "name = \"a\"\nexec = \"/bin/a\"\n");
    root.write("etc/services.d/b.toml", "name = \"b\"\nexec = \"/bin/b\"\n");
    assert!(set_enabled(root.path(), "a", false).unwrap());
    assert!(!set_enabled(root.path(), "a", false).unwrap());
    let text = fs::read_to_string(root.path().join("etc/raven/services.enable")).unwrap();
    assert_eq!(text, "b\n");
    assert!(set_enabled(root.path(), "a", true).unwrap());
    let text = fs::read_to_string(root.path().join("etc/raven/services.enable")).unwrap();
    assert_eq!(text, "a\nb\n");
    assert!(set_enabled(root.path(), "missing", true).is_err());
}
