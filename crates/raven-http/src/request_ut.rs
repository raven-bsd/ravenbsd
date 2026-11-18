// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Request-builder and wire-encoding tests.

use super::RequestBuilder;
use crate::url::Url;
use crate::{DEFAULT_MAX_BODY, DEFAULT_MAX_REDIRECTS, DEFAULT_TIMEOUT_MS, Error};
use alloc::string::String;

fn encode(request: &super::Request) -> String {
    let bytes = request.encode(request.url(), true);
    String::from_utf8(bytes).expect("utf-8 request")
}

#[test]
fn builds_a_complete_get_request() {
    let request = RequestBuilder::new("http://example.com/a?q=1")
        .expect("builder")
        .header("X-Test", "yes")
        .expect("header")
        .user_agent("eink/1")
        .expect("user-agent")
        .timeout_ms(1234)
        .max_redirects(2)
        .max_body(99)
        .build();
    assert_eq!(request.timeout_ms(), 1234);
    assert_eq!(request.max_redirects(), 2);
    assert_eq!(request.max_body(), 99);
    assert_eq!(request.headers().len(), 1);
    assert_eq!(
        encode(&request),
        "GET /a?q=1 HTTP/1.1\r\n\
         Host: example.com\r\n\
         User-Agent: eink/1\r\n\
         Accept: */*\r\n\
         Accept-Encoding: identity\r\n\
         Connection: close\r\n\
         X-Test: yes\r\n\
         \r\n"
    );
}

#[test]
fn defaults_are_bounded() {
    let request = RequestBuilder::new("http://example.com/")
        .expect("builder")
        .build();
    assert_eq!(request.timeout_ms(), DEFAULT_TIMEOUT_MS);
    assert_eq!(request.max_redirects(), DEFAULT_MAX_REDIRECTS);
    assert_eq!(request.max_body(), DEFAULT_MAX_BODY);
    assert!(encode(&request).contains("User-Agent: raven-http/0.1\r\n"));
}

#[test]
fn cross_origin_encoding_drops_credentials() {
    let request = RequestBuilder::new("http://example.com/")
        .expect("builder")
        .header("Authorization", "Bearer secret")
        .expect("auth")
        .header("Cookie", "session=1")
        .expect("cookie")
        .header("X-Keep", "yes")
        .expect("keep")
        .build();
    let encoded = request.encode(&Url::parse("http://other.test/").expect("url"), false);
    let text = String::from_utf8(encoded).expect("utf-8");
    assert!(!text.contains("Authorization"));
    assert!(!text.contains("Cookie"));
    assert!(!text.contains("secret"));
    assert!(text.contains("X-Keep: yes"));
    assert!(text.contains("Host: other.test"));
}

#[test]
fn rejects_reserved_and_malformed_headers() {
    for name in [
        "Host",
        "Connection",
        "Content-Length",
        "Transfer-Encoding",
        "User-Agent",
    ] {
        assert_eq!(
            RequestBuilder::new("http://example.com/")
                .expect("builder")
                .header(name, "x"),
            Err(Error::InvalidRequest),
            "{name}"
        );
    }
    assert_eq!(
        RequestBuilder::new("http://example.com/")
            .expect("builder")
            .header("Bad Name", "x"),
        Err(Error::InvalidRequest)
    );
    assert_eq!(
        RequestBuilder::new("http://example.com/")
            .expect("builder")
            .header("X-Test", "a\r\nb"),
        Err(Error::InvalidRequest)
    );
}

#[test]
fn rejects_https_and_malformed_urls() {
    assert_eq!(
        RequestBuilder::new("https://example.com/"),
        Err(Error::UnsupportedScheme)
    );
    assert_eq!(
        RequestBuilder::new("ftp://example.com/"),
        Err(Error::InvalidUrl)
    );
    assert_eq!(RequestBuilder::new(""), Err(Error::InvalidUrl));
}

#[test]
fn rejects_an_empty_or_control_user_agent() {
    assert_eq!(
        RequestBuilder::new("http://example.com/")
            .expect("builder")
            .user_agent(""),
        Err(Error::InvalidRequest)
    );
    assert_eq!(
        RequestBuilder::new("http://example.com/")
            .expect("builder")
            .user_agent("bad\r\nagent"),
        Err(Error::InvalidRequest)
    );
}
