// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the target-side strict network drop-in parser.

use super::*;

const STATIC: &str = "\
# static test\n\
[network]\n\
hostname = \"raven-static\"\n\
mode = \"static\"\n\
\n\
[network.static]\n\
address = \"10.0.2.15\"\n\
prefix = 24\n\
gateway = \"10.0.2.2\"\n\
dns = [\"10.0.2.3\", \"10.0.2.4\"]\n\
";

#[test]
fn static_configuration_parses() {
    let config = parse(STATIC).unwrap();
    assert_eq!(config.hostname.as_deref(), Some("raven-static"));
    assert_eq!(config.mode, NetworkMode::Static);
    let static_config = config.static_config().unwrap();
    assert_eq!(static_config.addr, [10, 0, 2, 15]);
    assert_eq!(static_config.prefix, 24);
    assert_eq!(static_config.gateway, Some([10, 0, 2, 2]));
    assert_eq!(static_config.dns, vec![[10, 0, 2, 3], [10, 0, 2, 4]]);
}

#[test]
fn dhcp_configuration_ignores_static_values() {
    let text = "\
[network]\n\
hostname = \"raven-dhcp\"\n\
mode = \"dhcp\"\n\
[network.static]\n\
address = \"10.0.9.9\"\n\
prefix = 24\n\
gateway = \"10.0.9.1\"\n\
";
    let config = parse(text).unwrap();
    assert_eq!(config.mode, NetworkMode::Dhcp);
    assert!(config.static_config().is_none());
}

#[test]
fn missing_mode_is_rejected() {
    assert!(parse("[network]\nhostname = \"raven\"\n").is_err());
}

#[test]
fn static_mode_needs_address_and_prefix() {
    assert!(parse("[network]\nmode = \"static\"\n").is_err());
    assert!(parse("[network]\nmode = \"static\"\n[network.static]\nprefix = 24\n").is_err());
    assert!(
        parse("[network]\nmode = \"static\"\n[network.static]\naddress = \"10.0.2.15\"\n").is_err()
    );
}

#[test]
fn bad_values_are_rejected_with_a_line() {
    let cases = [
        "[network]\nmode = \"auto\"\n",
        "[network]\nmode = \"static\"\n[network.static]\naddress = \"10.0.2.256\"\nprefix = 24\n",
        "[network]\nmode = \"static\"\n[network.static]\naddress = \"10.0.2.15\"\nprefix = 33\n",
        "[network]\nmode = \"static\"\n[network.static]\naddress = \"10.0.2.15\"\nprefix = 24\ngateway = \"0.0.0.0\"\n",
        "[network]\nmode = \"static\"\n[network.static]\naddress = \"10.0.2.15\"\nprefix = 24\ndns = []\n",
        "[network]\nmode = \"dhcp\"\nextra = 1\n",
        "[network]\nmode = \"dhcp\"\n[network.other]\n",
        "[other]\nmode = \"dhcp\"\n",
        "[network]\nmode = \"dhcp\"\nmode = \"dhcp\"\n",
    ];
    for text in cases {
        assert!(parse(text).is_err(), "accepted {text:?}");
    }
}

#[test]
fn hostname_rules_are_shared_with_the_config_crate() {
    assert!(validate_hostname("raven").is_ok());
    assert!(validate_hostname("raven-4g").is_ok());
    assert!(validate_hostname("").is_err());
    assert!(validate_hostname("bad name").is_err());
    assert!(validate_hostname("bad/name").is_err());
    assert!(validate_hostname(&"a".repeat(HOSTNAME_MAX + 1)).is_err());
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let text = "# leading\n\n[network] # section\nmode = \"dhcp\" # trailing\n";
    let config = parse(text).unwrap();
    assert_eq!(config.mode, NetworkMode::Dhcp);
}
