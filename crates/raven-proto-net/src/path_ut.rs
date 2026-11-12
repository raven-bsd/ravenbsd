// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `net:` path parsing and the accepted-socket path.

use super::*;

#[test]
fn parses_tcp_and_udp_paths() {
    assert_eq!(
        parse("net:tcp/10.0.0.2:8080"),
        Ok(SchemePath {
            transport: Transport::Tcp,
            endpoint: Some(Endpoint {
                ip: [10, 0, 0, 2],
                port: 8080
            }),
        })
    );
    assert_eq!(
        parse("net:udp/255.255.255.255:65535").map(|path| path.transport),
        Ok(Transport::Udp)
    );
}

#[test]
fn control_path_has_no_endpoint() {
    assert_eq!(
        parse("net:"),
        Ok(SchemePath {
            transport: Transport::Control,
            endpoint: None
        })
    );
}

#[test]
fn rejects_foreign_schemes() {
    assert_eq!(parse("file:/etc/passwd"), Err(PathError::NotNet));
    assert_eq!(parse(""), Err(PathError::NotNet));
}

#[test]
fn rejects_unknown_transports() {
    assert_eq!(parse("net:raw/1.2.3.4:1"), Err(PathError::UnknownTransport));
    assert_eq!(parse("net:tcp"), Err(PathError::UnknownTransport));
}

#[test]
fn rejects_bad_addresses() {
    for path in [
        "net:tcp/1.2.3:80",
        "net:tcp/1.2.3.4.5:80",
        "net:tcp/256.2.3.4:80",
        "net:tcp/1.2.3.a:80",
        "net:tcp/01.2.3.4:80",
        "net:tcp/:80",
        "net:tcp/1.2.3.4",
    ] {
        assert!(parse(path).is_err(), "{path} must be rejected");
    }
}

#[test]
fn accepted_path_is_numeric_and_bounded() {
    assert_eq!(parse_accepted("net:accept/0"), Some(0));
    assert_eq!(parse_accepted("net:accept/42"), Some(42));
    for path in [
        "net:accept/",
        "net:accept/01",
        "net:accept/+1",
        "net:accept/99999999999",
        "net:accept/1/2",
        "net:tcp/1.2.3.4:80",
    ] {
        assert_eq!(parse_accepted(path), None, "{path}");
    }
}

#[test]
fn rejects_bad_ports() {
    for path in [
        "net:tcp/1.2.3.4:0",
        "net:tcp/1.2.3.4:65536",
        "net:tcp/1.2.3.4:",
        "net:tcp/1.2.3.4:8o",
        "net:tcp/1.2.3.4:080",
        "net:tcp/1.2.3.4:80:90",
    ] {
        assert!(parse(path).is_err(), "{path} must be rejected");
    }
}
