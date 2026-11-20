// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the login policy.

use super::*;

const ROOT_SHADOW: &str = "root:$6$rounds=5000$G3aRZtaEHFI9mxqPo3H6$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.:0:0:99999:7:0:0:0";

#[test]
fn root_passwd_entry_is_selected() {
    let text = "nobody:x:65534:65534:nobody:/:/bin/false\nroot:x:0:0:root:/:/bin/rsh\n";
    let entry = parse_root_passwd(text).expect("root entry");
    assert_eq!(entry.name, "root");
    assert_eq!(entry.home, "/");
    // The root session shell is login policy; the passwd field stays account metadata.
    assert_eq!(entry.shell, DEFAULT_SHELL);
    assert_eq!(entry.shell, "/bin/brush");
}

#[test]
fn malformed_and_non_root_passwd_lines_are_rejected() {
    assert!(parse_root_passwd("root:x:0:0:root:/:/bin/rsh:extra\n").is_none());
    assert!(parse_root_passwd("nobody:x:65534:65534:nobody:/:/bin/false\n").is_none());
    assert!(parse_root_passwd("root:x:0:0:root::/bin/rsh\n").is_none());
    assert!(parse_root_passwd("").is_none());
}

#[test]
fn profile_values_override_defaults() {
    let profile = parse_profile(
        "# comment\n\nPATH=/usr/bin:/bin\nHOME=/root\nTERM=xterm\nUSER=root\nSHELL=/bin/brush\n",
    );
    assert_eq!(profile.path, "/usr/bin:/bin");
    assert_eq!(profile.home, "/root");
    assert_eq!(profile.term, "xterm");
    assert_eq!(profile.user, "root");
    assert_eq!(profile.shell, "/bin/brush");
}

#[test]
fn profile_ignores_unknown_and_malformed_lines() {
    let profile = parse_profile("PATH=/bin\nWHAT=ever\nNO_EQUALS\nTERM=\n");
    assert_eq!(profile.path, "/bin");
    assert_eq!(profile.home, DEFAULT_HOME);
    assert_eq!(profile.term, "");
}

#[test]
fn the_rel1b_default_hash_verifies_raven() {
    assert!(verify_root_password(ROOT_SHADOW, "raven"));
    assert!(!verify_root_password(ROOT_SHADOW, "raven!"));
    assert!(!verify_root_password(ROOT_SHADOW, ""));
    assert!(root_shadow_line(ROOT_SHADOW).is_some());
}

#[test]
fn login_status_returns_retry_for_a_wrong_password_and_ok_for_raven() {
    assert_eq!(login_status(ROOT_SHADOW, "root", "raven"), LOGIN_OK);
    assert_eq!(login_status(ROOT_SHADOW, "root", "wrong"), LOGIN_RETRY);
    assert_eq!(login_status(ROOT_SHADOW, "nobody", "raven"), LOGIN_RETRY);
    assert_eq!(login_status(ROOT_SHADOW, "root", ""), LOGIN_RETRY);
}

#[test]
fn other_accounts_and_locked_hashes_never_verify() {
    let other = "nobody:$6$rounds=5000$salt$hash:0:0:99999:7:0:0:0";
    assert!(!verify_root_password(other, "raven"));
    let locked = "root:!$6$rounds=5000$salt$hash:0:0:99999:7:0:0:0";
    assert!(!verify_root_password(locked, "raven"));
    let star = "root:*:0:0:99999:7:0:0:0";
    assert!(!verify_root_password(star, "raven"));
}

#[test]
fn any_account_with_a_shadow_record_can_authenticate() {
    let alice_hash = "$6$rounds=5000$G3aRZtaEHFI9mxqPo3H6$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.";
    let shadow = format!("{ROOT_SHADOW}\nalice:{alice_hash}:0:0:99999:7:0:0:0\n");
    assert_eq!(login_status(&shadow, "alice", "raven"), LOGIN_OK);
    assert_eq!(login_status(&shadow, "alice", "wrong"), LOGIN_RETRY);
    assert_eq!(login_status(&shadow, "bob", "raven"), LOGIN_RETRY);
}

#[test]
fn passwd_entries_are_looked_up_by_name() {
    let text =
        "root:x:0:0:root:/root:/bin/rsh\nalice:x:1000:1000:Alice:/home/alice:/bin/tui-demo\n";
    let entries = parse_passwd(text);
    assert_eq!(entries.len(), 2);
    let alice = find_passwd(text, "alice").expect("alice");
    assert_eq!(alice.home, "/home/alice");
    assert_eq!(alice.shell, "/bin/tui-demo");
    assert!(find_passwd(text, "bob").is_none());
}

#[test]
fn autologin_is_parsed_and_invalid_users_disable_it() {
    assert_eq!(
        parse_autologin("[console]\nuser = \"musicbox\"\nenabled = true\n"),
        Autologin {
            enabled: true,
            user: String::from("musicbox"),
        }
    );
    assert!(!parse_autologin("user = \"musicbox\"\nenabled = false\n").enabled);
    assert_eq!(
        parse_autologin("user = \"bad:name\"\nenabled = true\n"),
        Autologin::default()
    );
    assert_eq!(parse_autologin(""), Autologin::default());
}

#[test]
fn usernames_are_normalised_and_bounded() {
    assert_eq!(normalize_username("root\n"), Some("root"));
    assert_eq!(normalize_username("  root  "), Some("root"));
    assert_eq!(normalize_username("ro_ot-1"), Some("ro_ot-1"));
    assert_eq!(normalize_username(""), None);
    assert_eq!(normalize_username("root/"), None);
    assert_eq!(normalize_username("root;reboot"), None);
    let long = "a".repeat(MAX_USER + 1);
    assert_eq!(normalize_username(&long), None);
    assert!(is_root_user("root"));
    assert!(!is_root_user("nobody"));
}
