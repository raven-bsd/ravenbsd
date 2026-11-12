// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the mount table.

use super::*;

fn table() -> MountTable {
    let mut table = MountTable::new();
    table.mount("/", "initfs:").expect("root");
    table
}

#[test]
fn root_resolves_whole_tree() {
    let table = table();
    assert_eq!(table.resolve("/").expect("root"), (1, ""));
    assert_eq!(
        table.resolve("/etc/mounts").expect("file"),
        (1, "etc/mounts")
    );
    assert_eq!(table.resolve("/a/b/c").expect("deep"), (1, "a/b/c"));
}

#[test]
fn longest_prefix_wins() {
    let mut table = table();
    let tmp = table.mount("/tmp", "ramfs:").expect("tmp");
    let nested = table.mount("/tmp/x", "zero:").expect("nested");
    assert_eq!(table.resolve("/tmp/a").expect("tmp file"), (tmp, "a"));
    assert_eq!(table.resolve("/tmp").expect("tmp root"), (tmp, ""));
    assert_eq!(table.resolve("/tmp/x").expect("nested root"), (nested, ""));
    assert_eq!(
        table.resolve("/tmp/x/a").expect("nested file"),
        (nested, "a")
    );
    assert_eq!(table.resolve("/tmpx").expect("not tmp"), (1, "tmpx"));
}

#[test]
fn exact_mount_point_maps_to_empty_suffix() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.resolve("/tmp").expect("tmp"), (2, ""));
}

#[test]
fn missing_mount_is_enoent() {
    let table = MountTable::new();
    assert_eq!(table.resolve("/x"), Err(Errno::ENOENT));
}

#[test]
fn relative_or_empty_path_is_invalid() {
    let table = table();
    assert_eq!(table.resolve("a"), Err(Errno::EINVAL));
    assert_eq!(table.resolve(""), Err(Errno::EINVAL));
}

#[test]
fn non_normalized_mount_points_are_rejected() {
    let mut table = MountTable::new();
    assert_eq!(table.mount("tmp", "ramfs:"), Err(Errno::EINVAL));
    assert_eq!(table.mount("/a//b", "ramfs:"), Err(Errno::EINVAL));
    assert_eq!(table.mount("/a/", "ramfs:"), Err(Errno::EINVAL));
    assert_eq!(table.mount("/a/../b", "ramfs:"), Err(Errno::EINVAL));
}

#[test]
fn invalid_schemes_are_rejected() {
    let mut table = MountTable::new();
    for scheme in ["", "ramfs", ":ramfs", "a/b:", "a:b:", "x\0:"] {
        assert_eq!(table.mount("/", scheme), Err(Errno::EINVAL), "{scheme:?}");
    }
}

#[test]
fn duplicate_mount_is_eexist() {
    let mut table = table();
    assert_eq!(table.mount("/", "ramfs:"), Err(Errno::EEXIST));
}

#[test]
fn maximum_mounts_are_enforced() {
    let mut table = MountTable::new();
    for index in 0..MAX_MOUNTS {
        let path = alloc::format!("/m{index}");
        table.mount(&path, "ramfs:").expect("mount");
    }
    assert_eq!(table.len(), MAX_MOUNTS);
    assert_eq!(table.mount("/extra", "ramfs:"), Err(Errno::ENOSPC));
}

#[test]
fn mount_ids_are_stable_and_not_reused() {
    let mut table = table();
    let first = table.mount("/tmp", "ramfs:").expect("tmp");
    let second = table.mount("/var", "ramfs:").expect("var");
    table.umount("/tmp", 0).expect("umount");
    let third = table.mount("/tmp", "ramfs:").expect("remount");
    assert!(first < second && second < third);
    assert_eq!(table.get(first), None);
    assert_eq!(table.get(second).expect("var").path, "/var");
    assert_eq!(table.get(third).expect("tmp").path, "/tmp");
}

#[test]
fn umount_missing_is_enoent() {
    let mut table = table();
    assert_eq!(table.umount("/tmp", 0), Err(Errno::ENOENT));
}

#[test]
fn umount_root_with_others_is_ebusy() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.umount("/", 0), Err(Errno::EBUSY));
}

#[test]
fn umount_root_alone_is_allowed() {
    let mut table = table();
    table.umount("/", 0).expect("root");
    assert!(table.is_empty());
}

#[test]
fn umount_with_open_files_is_ebusy() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.umount("/tmp", 1), Err(Errno::EBUSY));
    table.umount("/tmp", 0).expect("tmp");
}

#[test]
fn umount_requires_normalized_path() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.umount("tmp", 0), Err(Errno::EINVAL));
    assert_eq!(table.umount("/tmp/", 0), Err(Errno::EINVAL));
}

#[test]
fn rename_same_mount_is_ok() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.rename("/tmp/a", "/tmp/b"), Ok(()));
}

#[test]
fn rename_across_mounts_is_exdev() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.rename("/tmp/a", "/etc/a"), Err(Errno::EXDEV));
}

#[test]
fn rename_missing_path_is_enoent() {
    let table = MountTable::new();
    assert_eq!(table.rename("/nope", "/also"), Err(Errno::ENOENT));
}

#[test]
fn mounts_below_root_lists_only_direct_children() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    table.mount("/dev/null", "null:").expect("null");
    table.mount("/tmp/x", "zero:").expect("nested");
    let children: Vec<&str> = table
        .mounts_below("/")
        .iter()
        .map(|mount| mount.path.as_str())
        .collect();
    assert_eq!(children, ["/tmp"]);
}

#[test]
fn mounts_below_nested_lists_direct_children() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    table.mount("/tmp/x", "zero:").expect("x");
    table.mount("/tmp/x/y", "rand:").expect("y");
    let children: Vec<&str> = table
        .mounts_below("/tmp")
        .iter()
        .map(|mount| mount.path.as_str())
        .collect();
    assert_eq!(children, ["/tmp/x"]);
}

#[test]
fn mounts_below_missing_path_is_empty() {
    let table = table();
    assert!(table.mounts_below("/none").is_empty());
}

#[test]
fn defaults_are_mounted() {
    let mut table = MountTable::new();
    assert_eq!(table.mount_defaults(), Ok(DEFAULT_MOUNTS.len()));
    assert_eq!(table.len(), DEFAULT_MOUNTS.len());
    assert_eq!(table.resolve("/tmp/a").expect("tmp").0, 2);
    assert_eq!(table.resolve("/dev/null").expect("null").0, 3);
}

#[test]
fn parse_mounts_accepts_comments_and_blank_lines() {
    let parsed = parse_mounts("# table\n\n/ initfs:\n/tmp ramfs: # trailing\n/dev/null null:\n")
        .expect("parse");
    assert_eq!(
        parsed,
        vec![
            ("/".to_string(), "initfs:".to_string()),
            ("/tmp".to_string(), "ramfs:".to_string()),
            ("/dev/null".to_string(), "null:".to_string()),
        ]
    );
}

#[test]
fn parse_mounts_rejects_bad_lines() {
    assert_eq!(parse_mounts("/\n"), Err(Errno::EINVAL));
    assert_eq!(parse_mounts("/ initfs: extra\n"), Err(Errno::EINVAL));
    assert_eq!(parse_mounts("tmp ramfs:\n"), Err(Errno::EINVAL));
    assert_eq!(parse_mounts("/ ramfs\n"), Err(Errno::EINVAL));
}

#[test]
fn parse_mounts_rejects_duplicate_paths() {
    assert_eq!(parse_mounts("/ initfs:\n/ ramfs:\n"), Err(Errno::EEXIST));
}

#[test]
fn parse_mounts_normalizes_nothing() {
    assert_eq!(parse_mounts("// initfs:\n"), Err(Errno::EINVAL));
}

#[test]
fn normalized_rejects_relative_and_normalizes_dots() {
    assert_eq!(normalized("a"), Err(Errno::EINVAL));
    assert_eq!(normalized("/a/./b/..").expect("normalized"), "/a");
}

#[test]
fn valid_scheme_requires_trailing_colon() {
    assert!(valid_scheme("ramfs:"));
    assert!(valid_scheme("a-b_c:"));
    for scheme in ["", ":", "ramfs", "a/b:", "a:b:"] {
        assert!(!valid_scheme(scheme), "{scheme:?}");
    }
}

#[test]
fn root_only_table_resolves_root() {
    let table = table();
    assert_eq!(table.resolve("/").expect("root"), (1, ""));
    assert_eq!(table.resolve("/x").expect("x"), (1, "x"));
}

#[test]
fn nested_mount_is_reachable_after_parent_removal() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    let nested = table.mount("/tmp/x", "zero:").expect("nested");
    table.umount("/tmp", 0).expect("umount");
    assert_eq!(table.resolve("/tmp/x/a").expect("nested"), (nested, "a"));
}

#[test]
fn mount_scheme_is_returned() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.get(2).expect("tmp").scheme, "ramfs:");
}

#[test]
fn mount_id_overflow_is_enfile() {
    let mut table = MountTable::new();
    table.next_id = u32::MAX - 1;
    assert_eq!(table.mount("/a", "ramfs:"), Ok(u32::MAX - 1));
    assert_eq!(table.mount("/b", "ramfs:"), Err(Errno::ENFILE));
}

#[test]
fn mount_below_exact_equals_itself() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    table.mount("/tmp/x", "zero:").expect("nested");
    assert!(table.mounts_below("/tmp").iter().all(|m| m.path != "/tmp"));
}

#[test]
fn default_table_is_empty_and_starts_at_one() {
    let mut table = MountTable::default();
    assert!(table.is_empty());
    assert_eq!(table.mount("/", "initfs:"), Ok(1));
}

#[test]
fn prefix_without_a_component_boundary_does_not_match() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.resolve("/tm").expect("root path"), (1, "tm"));
}

#[test]
fn parse_mounts_empty_text_is_empty() {
    assert_eq!(parse_mounts(""), Ok(Vec::new()));
    assert_eq!(parse_mounts("  \n# only\n"), Ok(Vec::new()));
}

#[test]
fn parse_mounts_inline_comment_is_ignored() {
    assert_eq!(
        parse_mounts("/ initfs: # root\n"),
        Ok(vec![("/".to_string(), "initfs:".to_string())])
    );
}

#[test]
fn umount_one_mount_keeps_the_others() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    table.mount("/var", "ramfs:").expect("var");
    table.umount("/tmp", 0).expect("tmp");
    assert_eq!(table.resolve("/var/a").expect("var").0, 3);
    assert_eq!(table.resolve("/tmp/a").expect("root").0, 1);
}

#[test]
fn mounts_iterator_is_in_id_order() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    table.mount("/var", "ramfs:").expect("var");
    let ids: Vec<u32> = table.mounts().map(|mount| mount.id).collect();
    assert_eq!(ids, [1, 2, 3]);
}

#[test]
fn rename_same_path_is_ok() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.rename("/tmp/a", "/tmp/a"), Ok(()));
}

#[test]
fn rename_exact_mount_points_on_different_mounts_is_exdev() {
    let mut table = table();
    table.mount("/tmp", "ramfs:").expect("tmp");
    assert_eq!(table.rename("/tmp", "/"), Err(Errno::EXDEV));
}

#[test]
fn mount_defaults_twice_is_eexist() {
    let mut table = MountTable::new();
    table.mount_defaults().expect("defaults");
    assert_eq!(table.mount_defaults(), Err(Errno::EEXIST));
}
