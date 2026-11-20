// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the strict `[network]` schema and the atomic writer.

use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-config-network-{name}-{}-{}",
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
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn static_config() -> NetworkConfig {
    NetworkConfig::statically(
        Some(String::from("raven-static")),
        Ipv4Addr::new(10, 0, 2, 15),
        24,
        Some(Ipv4Addr::new(10, 0, 2, 2)),
        vec![Ipv4Addr::new(10, 0, 2, 3)],
    )
}

fn with_network_key(mut node: Node, key: &str, value: Node) -> Node {
    if let Node::Table(table) = &mut node {
        table.insert(key.to_string(), value);
    }
    node
}

fn with_static_key(mut node: Node, key: &str, value: Node) -> Node {
    if let Node::Table(table) = &mut node
        && let Some(Node::Table(static_table)) = table.get_mut("static")
    {
        static_table.insert(key.to_string(), value);
    }
    node
}

#[test]
fn static_section_round_trips_through_node_and_text() {
    let config = static_config();
    assert_eq!(from_node(&config.to_node()).unwrap(), config);
    let text = config.render();
    assert!(text.contains("[network]"));
    assert!(text.contains("hostname = \"raven-static\""));
    assert!(text.contains("mode = \"static\""));
    assert!(text.contains("address = \"10.0.2.15\""));
    assert!(text.contains("prefix = 24"));
    assert!(text.contains("gateway = \"10.0.2.2\""));
    assert!(text.contains("dns = [\"10.0.2.3\"]"));
}

#[test]
fn dhcp_section_is_valid_without_static_parameters() {
    let config = NetworkConfig::dhcp(Some(String::from("raven")));
    assert_eq!(from_node(&config.to_node()).unwrap(), config);
    assert!(config.render().contains("mode = \"dhcp\""));
}

#[test]
fn dhcp_mode_may_carry_but_ignores_static_values() {
    let config = NetworkConfig {
        hostname: None,
        mode: NetworkMode::Dhcp,
        static_network: static_config().static_network,
    };
    let parsed = from_node(&config.to_node()).unwrap();
    assert_eq!(parsed.mode, NetworkMode::Dhcp);
    assert_eq!(
        parsed.static_network.unwrap().address,
        Ipv4Addr::new(10, 0, 2, 15)
    );
}

#[test]
fn static_mode_requires_an_address() {
    let node = NetworkConfig {
        hostname: None,
        mode: NetworkMode::Static,
        static_network: None,
    };
    let error = from_node(&node.to_node()).unwrap_err();
    assert!(error.contains("static"));
    assert!(error.contains("address"));
}

#[test]
fn empty_and_zero_gateway_are_rejected() {
    let mut config = static_config();
    config.static_network.as_mut().unwrap().gateway = Some(Ipv4Addr::UNSPECIFIED);
    assert!(config.validate().is_err());

    let node = with_static_key(
        static_config().to_node(),
        "gateway",
        Node::String(String::new()),
    );
    assert!(from_node(&node).is_err());

    let node = with_static_key(
        static_config().to_node(),
        "gateway",
        Node::String(String::from("0.0.0.0")),
    );
    assert!(from_node(&node).is_err());
}

#[test]
fn unknown_keys_and_bad_values_are_rejected() {
    let node = with_network_key(
        NetworkConfig::dhcp(None).to_node(),
        "nope",
        Node::Integer(1),
    );
    assert!(from_node(&node).unwrap_err().contains("unknown"));

    let node = with_network_key(
        NetworkConfig::dhcp(None).to_node(),
        "mode",
        Node::String(String::from("auto")),
    );
    assert!(from_node(&node).unwrap_err().contains("dhcp"));

    let mut config = static_config();
    config.static_network.as_mut().unwrap().prefix = 33;
    assert!(config.validate().is_err());

    let mut config = static_config();
    config.hostname = Some(String::from("bad name"));
    assert!(config.validate().is_err());
}

#[test]
fn writer_is_atomic_and_creates_the_drop_in_directory() {
    let root = TempDir::new("writer");
    let path = write_drop_in(root.path(), &static_config()).unwrap();
    assert_eq!(path, root.path().join(NETWORK_DROP_IN));
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("mode = \"static\""));
    let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .collect();
    assert!(leftovers.is_empty());
}
