// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the network form model and the atomic drop-in writer.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    Current, Field, MODE_CHOICES, NetworkCurrent, load_network, modules, network_path,
    plan_from_fields, write_atomic,
};

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-tui-model-{}-{}",
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
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path.join(relative)).unwrap()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn fields_for(current: &NetworkCurrent) -> Vec<Field> {
    modules(&Current {
        network: current.clone(),
        accounts: raven_config::users::Accounts::default(),
    })
    .remove(0)
    .fields
}

fn set(fields: &mut [Field], key: &str, value: &str) {
    let field = fields.iter_mut().find(|field| field.key == key).unwrap();
    field.value = value.to_string();
}

#[test]
fn dhcp_plan_has_no_static_block() {
    let mut fields = fields_for(&NetworkCurrent::default());
    set(&mut fields, "hostname", "raven-tui");
    let plan = plan_from_fields(&fields).unwrap();
    assert_eq!(plan.hostname, "raven-tui");
    assert_eq!(plan.mode, "dhcp");
    let text = plan.to_toml();
    assert!(text.contains("[network]"));
    assert!(text.contains("hostname = \"raven-tui\""));
    assert!(text.contains("mode = \"dhcp\""));
    assert!(!text.contains("[network.static]"));
}

#[test]
fn static_plan_round_trips_the_k198_schema() {
    let mut fields = fields_for(&NetworkCurrent::default());
    set(&mut fields, "hostname", "router");
    set(&mut fields, "mode", "static");
    set(&mut fields, "address", "10.0.2.15");
    set(&mut fields, "prefix", "24");
    set(&mut fields, "gateway", "10.0.2.2");
    set(&mut fields, "dns", "10.0.2.3, 1.1.1.1");
    let plan = plan_from_fields(&fields).unwrap();
    assert_eq!(plan.mode, "static");
    let text = plan.to_toml();
    assert!(text.contains("[network.static]"));
    assert!(text.contains("address = \"10.0.2.15\""));
    assert!(text.contains("prefix = 24"));
    assert!(text.contains("gateway = \"10.0.2.2\""));
    assert!(text.contains("dns = [\"10.0.2.3\", \"1.1.1.1\"]"));
}

#[test]
fn static_requires_address_gateway_and_prefix() {
    let mut fields = fields_for(&NetworkCurrent::default());
    set(&mut fields, "mode", "static");
    set(&mut fields, "prefix", "0");
    let errors = plan_from_fields(&fields).unwrap_err();
    let keys: Vec<&str> = errors.iter().map(|error| error.key).collect();
    assert!(keys.contains(&"address"));
    assert!(keys.contains(&"prefix"));
    assert!(keys.contains(&"gateway"));
}

#[test]
fn hostname_and_ipv4_are_validated() {
    let mut fields = fields_for(&NetworkCurrent::default());
    set(&mut fields, "hostname", "-bad-");
    set(&mut fields, "mode", "static");
    set(&mut fields, "address", "999.1.1.1");
    set(&mut fields, "gateway", "10.0.2.2");
    let errors = plan_from_fields(&fields).unwrap_err();
    let keys: Vec<&str> = errors.iter().map(|error| error.key).collect();
    assert!(keys.contains(&"hostname"));
    assert!(keys.contains(&"address"));
}

#[test]
fn mode_choices_are_the_k198_modes() {
    assert!(MODE_CHOICES.contains(&"dhcp"));
    assert!(MODE_CHOICES.contains(&"static"));
}

#[test]
fn write_atomic_writes_only_the_dedicated_drop_in() {
    let root = TempDir::new();
    root.write("etc/raven/raven.toml", "keep = true\n");
    let mut fields = fields_for(&NetworkCurrent::default());
    set(&mut fields, "hostname", "atomic");
    let plan = plan_from_fields(&fields).unwrap();
    let path = write_atomic(root.path(), &plan).unwrap();
    assert_eq!(path, network_path(root.path()));
    let text = root.read("etc/raven/conf.d/network.toml");
    assert!(text.contains("hostname = \"atomic\""));
    assert!(text.contains("mode = \"dhcp\""));
    assert_eq!(root.read("etc/raven/raven.toml"), "keep = true\n");
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn load_network_reads_the_drop_in() {
    let root = TempDir::new();
    root.write(
        "etc/raven/conf.d/network.toml",
        "[network]\nhostname = \"router\"\nmode = \"static\"\n\
         [network.static]\naddress = \"10.0.2.15\"\nprefix = 24\n\
         gateway = \"10.0.2.2\"\ndns = [\"10.0.2.3\", \"1.1.1.1\"]\n",
    );
    let (current, warning) = load_network(root.path());
    assert!(warning.is_none());
    assert_eq!(current.hostname, "router");
    assert_eq!(current.mode, "static");
    assert_eq!(current.address, "10.0.2.15");
    assert_eq!(current.prefix, "24");
    assert_eq!(current.gateway, "10.0.2.2");
    assert_eq!(current.dns, vec!["10.0.2.3", "1.1.1.1"]);
}

#[test]
fn malformed_drop_in_is_a_warning_not_a_crash() {
    let root = TempDir::new();
    root.write("etc/raven/conf.d/network.toml", "[network]\nhostname = 7\n");
    let (current, warning) = load_network(root.path());
    assert_eq!(current, NetworkCurrent::default());
    assert!(warning.is_some());
}
