// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP signature option plumbing (RFC 2385/5925). No digest is implemented here.
//!
//! The digest backend is deliberately behind a trait and is not selected until
//! the reviewer approves a crate; see `tickets/carl/reports/C18.md`.

use alloc::vec::Vec;

/// A TCP-AO key and algorithm selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AoKey {
    /// Key identifier carried in the option.
    pub key_id: u8,
    /// MAC algorithm.
    pub algorithm: AoAlgorithm,
    /// Key material.
    pub key: Vec<u8>,
}

/// TCP-AO MAC algorithms from RFC 5926.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AoAlgorithm {
    /// HMAC-SHA-1.
    HmacSha1,
    /// HMAC-SHA-256.
    HmacSha256,
}

/// The digest operations C18 needs. Implementations require reviewer approval.
pub trait DigestProvider {
    /// MD5 digest for RFC 2385.
    fn md5(&self, data: &[u8]) -> [u8; 16];
    /// HMAC-SHA-1 for TCP-AO.
    fn hmac_sha1(&self, key: &[u8], data: &[u8]) -> [u8; 20];
    /// HMAC-SHA-256 for TCP-AO.
    fn hmac_sha256(&self, key: &[u8], data: &[u8]) -> [u8; 32];
}

/// The default provider: no cryptography is available until one is approved.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDigest;

impl DigestProvider for NoDigest {
    fn md5(&self, _data: &[u8]) -> [u8; 16] {
        [0; 16]
    }
    fn hmac_sha1(&self, _key: &[u8], _data: &[u8]) -> [u8; 20] {
        [0; 20]
    }
    fn hmac_sha256(&self, _key: &[u8], _data: &[u8]) -> [u8; 32] {
        [0; 32]
    }
}
