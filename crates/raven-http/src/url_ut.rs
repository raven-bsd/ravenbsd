// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! URL parsing and reference-resolution tests.

use super::{Scheme, Url};
use crate::Error;
use alloc::string::ToString;

#[test]
fn parses_plain_http_urls() {
    let url = Url::parse("http://Example.COM").expect("url");
    assert_eq!(url.scheme, Scheme::Http);
    assert_eq!(url.host, "example.com");
    assert_eq!(url.port, 80);
    assert_eq!(url.path, "/");

    let url = Url::parse("http://example.com:8080/a/b?q=1#frag").expect("url");
    assert_eq!(url.port, 8080);
    assert_eq!(url.path, "/a/b?q=1");
    assert_eq!(url.host_header(), "example.com:8080");
}

#[test]
fn parses_https_for_redirect_policy() {
    let url = Url::parse("https://example.com/x").expect("url");
    assert_eq!(url.scheme, Scheme::Https);
    assert_eq!(url.port, 443);
    assert_eq!(url.path, "/x");
}

#[test]
fn parses_ipv4_and_bracketed_ipv6() {
    let url = Url::parse("http://192.0.2.10:8000/status").expect("url");
    assert_eq!(url.host, "192.0.2.10");
    assert_eq!(url.port, 8000);

    let url = Url::parse("http://[2001:DB8::1]/").expect("url");
    assert_eq!(url.host, "[2001:db8::1]");
    assert_eq!(url.port, 80);
    assert_eq!(url.host_header(), "[2001:db8::1]");
}

#[test]
fn rejects_bad_urls() {
    for input in [
        "",
        "ftp://example.com/",
        "http://",
        "http:///x",
        "http://user@example.com/",
        "http://exa mple.com/",
        "http://example.com:0/",
        "http://example.com:99999/",
        "http://2001:db8::1/",
        "http://example.com/pa\rth",
    ] {
        assert_eq!(Url::parse(input), Err(Error::InvalidUrl), "{input}");
    }
}

#[test]
fn resolves_absolute_and_network_path_references() {
    let base = Url::parse("http://example.com/a/b?x=1").expect("base");
    let absolute = base.resolve("https://other.test/y").expect("resolve");
    assert_eq!(absolute.scheme, Scheme::Https);
    assert_eq!(absolute.host, "other.test");
    assert_eq!(absolute.path, "/y");

    let network = base.resolve("//other.test:8080/z").expect("resolve");
    assert_eq!(network.scheme, Scheme::Http);
    assert_eq!(network.host, "other.test");
    assert_eq!(network.port, 8080);
    assert_eq!(network.path, "/z");
}

#[test]
fn resolves_paths_queries_and_dot_segments() {
    let base = Url::parse("http://example.com/a/b/c?old=1").expect("base");
    assert_eq!(base.resolve("/root").expect("resolve").path, "/root");
    assert_eq!(base.resolve("next").expect("resolve").path, "/a/b/next");
    assert_eq!(base.resolve("../up").expect("resolve").path, "/a/up");
    assert_eq!(base.resolve("./same").expect("resolve").path, "/a/b/same");
    assert_eq!(
        base.resolve("?new=2").expect("resolve").path,
        "/a/b/c?new=2"
    );
    assert_eq!(
        base.resolve("next?q=1").expect("resolve").path,
        "/a/b/next?q=1"
    );
    assert_eq!(base.resolve("#frag").expect("resolve").path, "/a/b/c?old=1");
    assert_eq!(base.resolve("").expect("resolve").path, "/a/b/c?old=1");
    assert_eq!(base.resolve("/a//b").expect("resolve").path, "/a//b");
}

#[test]
fn resolves_relative_url_into_directory_with_trailing_slash() {
    let base = Url::parse("http://example.com/a/b/").expect("base");
    assert_eq!(base.resolve("c").expect("resolve").path, "/a/b/c");
    assert_eq!(base.resolve("..").expect("resolve").path, "/a/");
    assert_eq!(base.resolve("../../../../x").expect("resolve").path, "/x");
}

#[test]
fn host_header_omits_the_default_port_only() {
    let url = Url::parse("http://example.com:80/").expect("url");
    assert_eq!(url.host_header(), "example.com");
    let url = Url::parse("https://example.com:443/").expect("url");
    assert_eq!(url.host_header(), "example.com");
    let url = Url::parse("http://example.com:81/").expect("url");
    assert_eq!(url.host_header(), "example.com:81");
}

#[test]
fn scheme_text_is_stable() {
    assert_eq!(Scheme::Http.as_str().to_string(), "http");
    assert_eq!(Scheme::Https.as_str().to_string(), "https");
}
