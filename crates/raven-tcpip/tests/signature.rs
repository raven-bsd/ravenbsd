// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C18 signature plumbing tests. Cryptography is intentionally absent.

#![cfg(feature = "testutil")]
#![allow(unused_imports)] // Imports are used by the feature-gated tests.

use raven_abi::errno::Errno;
use raven_tcpip::{AoAlgorithm, AoKey, DigestProvider, NoDigest};

#[cfg(not(feature = "tcp-md5"))]
#[test]
fn md5_setter_reports_not_supported_until_approved() {
    let mut stack = raven_tcpip::Stack::new(
        raven_tcpip::LoopbackDevice::new(),
        raven_tcpip::StackConfig::default(),
        1,
        raven_tcpip::Instant::ZERO,
    );
    assert_eq!(
        stack.set_md5_key(raven_tcpip::SocketId(1), Some(b"key")),
        Err(Errno::EOPNOTSUPP)
    );
}

#[cfg(not(feature = "tcp-ao"))]
#[test]
fn ao_setter_reports_not_supported_until_approved() {
    let mut stack = raven_tcpip::Stack::new(
        raven_tcpip::LoopbackDevice::new(),
        raven_tcpip::StackConfig::default(),
        1,
        raven_tcpip::Instant::ZERO,
    );
    let keys = [AoKey {
        key_id: 1,
        algorithm: AoAlgorithm::HmacSha256,
        key: vec![1, 2, 3],
    }];
    assert_eq!(
        stack.set_ao_keys(raven_tcpip::SocketId(1), &keys),
        Err(Errno::EOPNOTSUPP)
    );
}

#[test]
fn the_no_digest_provider_is_explicitly_placeholder() {
    let digest = NoDigest;
    assert_eq!(digest.md5(b"data"), [0u8; 16]);
    assert_eq!(digest.hmac_sha1(b"key", b"data"), [0u8; 20]);
    assert_eq!(digest.hmac_sha256(b"key", b"data"), [0u8; 32]);
}
