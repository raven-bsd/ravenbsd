// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Publisher-authority policy tests.
//!
//! The kernel test target is a `no_std` binary at this base and is not a runnable host test; the
//! executable red/green proof is the `scheme-publisher` qtest. These pure cases keep the policy
//! table and the restart rule explicit for review.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_names_cover_the_boot_services() {
        for name in ["file", "vfs", "tty", "net", "initfs"] {
            assert!(protected_name(name), "{name}");
        }
        assert!(!protected_name("echo"));
        assert!(!protected_name("evil"));
    }

    #[test]
    fn an_unprivileged_publisher_is_denied_before_and_after_a_record() {
        assert!(!publisher_allowed("file", 1000, None));
        assert!(!publisher_allowed("file", 1000, Some(0)));
        assert!(!publisher_allowed("vfs", 1000, Some(0)));
    }

    #[test]
    fn a_root_publisher_can_claim_and_reclaim() {
        assert!(publisher_allowed("file", 0, None));
        assert!(publisher_allowed("file", 0, Some(0)));
        assert!(publisher_allowed("vfs", 0, Some(0)));
    }

    #[test]
    fn an_unreserved_name_remains_open() {
        assert!(publisher_allowed("echo", 1000, None));
        assert!(publisher_allowed("s33", 1000, Some(0)));
    }

    #[test]
    fn executable_sources_are_the_three_boot_filesystems() {
        assert_eq!(EXEC_SOURCES, &["file", "initfs", "vfs"]);
    }
}
