// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `pthread_np` alias table (alias-only until C57 merges).

use super::*;

#[test]
fn table_matches_02_b09() {
    assert_eq!(
        ALIASES,
        &[
            ("pthread_set_name_np", "pthread_setname_np"),
            ("pthread_get_name_np", "pthread_getname_np"),
            ("pthread_attr_get_np", "pthread_getattr_np"),
            ("pthread_main_np", "-"),
            ("pthread_suspend_np", "ENOSYS"),
            ("pthread_resume_np", "ENOSYS"),
        ]
    );
}

#[test]
fn base_lookup() {
    assert_eq!(base("pthread_set_name_np"), Some("pthread_setname_np"));
    assert_eq!(base("pthread_suspend_np"), Some("ENOSYS"));
    assert_eq!(base("pthread_unknown"), None);
}

#[test]
fn no_duplicate_names() {
    for (index, (name, _)) in ALIASES.iter().enumerate() {
        for (other, _) in ALIASES.iter().skip(index + 1) {
            assert_ne!(name, other);
        }
    }
}
