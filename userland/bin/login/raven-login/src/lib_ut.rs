// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the login retry status contract.

use super::*;

const ROOT_SHADOW: &str = "root:$6$rounds=5000$G3aRZtaEHFI9mxqPo3H6$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.:0:0:99999:7:0:0:0";

#[test]
fn a_wrong_password_returns_the_retry_status() {
    assert_eq!(login_status(ROOT_SHADOW, "root", "wrong"), LOGIN_RETRY);
    assert_ne!(LOGIN_RETRY, LOGIN_OK);
}

#[test]
fn an_unknown_account_returns_the_retry_status() {
    assert_eq!(login_status(ROOT_SHADOW, "nobody", "raven"), LOGIN_RETRY);
    assert!(!is_root_user("nobody"));
}

#[test]
fn root_with_the_release_password_returns_ok() {
    assert_eq!(login_status(ROOT_SHADOW, "root", "raven"), LOGIN_OK);
}

#[test]
fn login_names_are_normalised_before_authentication() {
    assert_eq!(normalize_username(" root\n"), Some("root"));
    assert_eq!(normalize_username("root/"), None);
}
