// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_default_hash_verifies_the_release_password() {
    assert!(verify_password("raven", DEFAULT_ROOT_HASH));
    assert!(!verify_password("raven!", DEFAULT_ROOT_HASH));
}

#[test]
fn the_default_hash_is_a_sha512_crypt_record() {
    assert!(DEFAULT_ROOT_HASH.starts_with("$6$"));
    assert_eq!(default_root_hash(), DEFAULT_ROOT_HASH);
}

#[test]
fn hash_password_is_deterministic() {
    let first = hash_password("raven", DEFAULT_ROOT_SALT).expect("hash");
    let second = hash_password("raven", DEFAULT_ROOT_SALT).expect("hash");
    assert_eq!(first, second);
    assert!(verify_password("raven", &first));
    assert!(!verify_password("Raven", &first));
}

#[test]
fn a_malformed_hash_is_rejected() {
    assert!(!verify_password("raven", ""));
    assert!(!verify_password("raven", "not-a-hash"));
    assert!(!verify_password("raven", "$6$broken"));
}
