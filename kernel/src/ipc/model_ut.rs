// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the scheme-IPC policy model.

use super::*;
use alloc::format;

#[test]
fn scheme_names_follow_the_contract() {
    for name in ["echo", "file", "net", "a", "a-b_c:0"] {
        assert!(valid_scheme_name(name), "{name}");
    }
    for name in ["", "Echo", "a b", "a/b", "ä", &"a".repeat(65)] {
        assert!(!valid_scheme_name(name), "{name:?}");
    }
}

#[test]
fn paths_split_at_the_first_colon() {
    assert_eq!(split_path("echo:hello"), Ok(("echo", "hello")));
    assert_eq!(split_path("echo:"), Ok(("echo", "")));
    assert_eq!(split_path("file:/etc/passwd"), Ok(("file", "/etc/passwd")));
    assert_eq!(split_path("echo"), Err(PathError::MissingScheme));
    assert_eq!(split_path(""), Err(PathError::BadLength));
    assert_eq!(split_path("Echo:x"), Err(PathError::BadScheme));
    assert_eq!(
        split_path(&format!("echo:{}", "x".repeat(PATH_MAX))),
        Err(PathError::BadLength)
    );
}

#[test]
fn root_and_child_namespaces() {
    let root = Namespace::root();
    assert!(root.allows("anything"));
    let registered = alloc::vec!["echo".to_string()];
    let child = root.child_snapshot(&registered);
    assert!(!child.wildcard);
    assert!(child.allows("echo"));
    assert!(!child.allows("file"));

    let mut child = child;
    child.add("file");
    assert!(child.allows("file"));
    child.add("file");
    assert_eq!(child.names.len(), 2);
    let grandchild = child.child_snapshot(&[]);
    assert!(grandchild.allows("echo"));
    assert!(grandchild.allows("file"));

    let mut prefixed = Namespace::default();
    prefixed.allow_prefix("net");
    assert!(prefixed.allows("net"));
    assert!(prefixed.allows("net-tcp"));
    assert!(!prefixed.allows("file"));
    let inherited = prefixed.child_snapshot(&[]);
    assert!(inherited.allows("net-tcp"));
}

#[test]
fn prefixes_grant_only_inside_the_parent() {
    let root = Namespace::root();
    assert!(root.can_grant_prefix("net"));
    assert!(root.can_grant_prefix("file"));

    let mut parent = Namespace::default();
    parent.allow_prefix("net");
    assert!(parent.can_grant_prefix("net"));
    assert!(parent.can_grant_prefix("net-tcp"));
    assert!(!parent.can_grant_prefix("file"));
    assert!(!parent.can_grant_prefix("ne"));
    assert!(!parent.can_grant_prefix(""));

    let mut exact = Namespace::default();
    exact.add("net");
    assert!(exact.allows("net"));
    // An exact name must not be generalized into a prefix that also admits net2/net-tcp.
    assert!(!exact.can_grant_prefix("net"));
}

#[test]
fn prefixes_add_remove_and_clear_are_idempotent() {
    let mut namespace = Namespace::default();
    namespace.allow_prefix("net");
    namespace.allow_prefix("net");
    assert_eq!(namespace.prefixes, alloc::vec!["net".to_string()]);
    assert!(namespace.allows("net-tcp"));

    namespace.remove_prefix("net");
    assert!(!namespace.allows("net-tcp"));
    namespace.remove_prefix("net");
    assert!(namespace.prefixes.is_empty());

    namespace.allow_prefix("net");
    namespace.allow_prefix("file");
    namespace.clear_prefixes();
    assert!(namespace.prefixes.is_empty());
    assert!(!namespace.allows("net-tcp"));
}

#[test]
fn open_rights_are_derived_from_flags() {
    let read = open_rights(OPEN_READ).unwrap();
    assert!(read.contains(Rights::READ));
    assert!(!read.contains(Rights::WRITE));
    assert!(read.contains(Rights::DUP));

    let write = open_rights(OPEN_WRITE).unwrap();
    assert!(write.contains(Rights::WRITE));
    assert!(!write.contains(Rights::READ));

    let both = open_rights(OPEN_READ | OPEN_WRITE | OPEN_CREATE).unwrap();
    assert!(both.contains(Rights::READ.union(Rights::WRITE).union(Rights::DUP)));

    assert_eq!(open_rights(0), None);
    assert_eq!(open_rights(8), None);
}

#[test]
fn status_range_is_positive_errno_only() {
    assert!(valid_status(0));
    assert!(valid_status(5));
    assert!(valid_status(4095));
    assert!(!valid_status(-1));
    assert!(!valid_status(4096));
}

#[test]
fn random_paths_never_panic() {
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut text = String::new();
    for _ in 0..10_000 {
        text.clear();
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let length = (state % 32) as usize;
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            text.push(char::from(b'a' + (state % 26) as u8));
        }
        let _ = valid_scheme_name(&text);
        let _ = split_path(&text);
    }
}
