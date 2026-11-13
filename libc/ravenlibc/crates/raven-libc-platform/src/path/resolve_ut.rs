// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Path resolver table tests (>= 80 cases) and the `must_be_dir` family rules.

use alloc::vec;
use alloc::vec::Vec;

use crate::path::resolve::{DEFAULT_LIMITS, Limits, PathReason, file_path, resolve};
use crate::path::special::{SpecialName, special_fd, target_fd};

const OK: &[(&str, &str, bool, &str, bool)] = &[
    ("/", "a", false, "/a", false),
    ("/", "a/b", false, "/a/b", false),
    ("/", "//a", false, "/a", false),
    ("/", "///a///b", false, "/a/b", false),
    ("/", "a//b", false, "/a/b", false),
    ("/", "/a//b", false, "/a/b", false),
    ("/", "/a/./b", false, "/a/b", false),
    ("/", "/a/../b", false, "/b", false),
    ("/", "/a/b/..", false, "/a", true),
    ("/", "/a/b/.", false, "/a/b", true),
    ("/", "/a/b/", false, "/a/b", true),
    ("/", "/a/b//", false, "/a/b", true),
    ("/", "/..", false, "/", true),
    ("/", "/../", false, "/", true),
    ("/", "/../../x", false, "/x", false),
    ("/", "..", false, "/", true),
    ("/", ".", false, "/", true),
    ("/", "./", false, "/", true),
    ("/", ".hidden", false, "/.hidden", false),
    ("/", "..hidden", false, "/..hidden", false),
    ("/", "...", false, "/...", false),
    ("/tmp", "a", false, "/tmp/a", false),
    ("/tmp", "a/b", false, "/tmp/a/b", false),
    ("/tmp", ".", false, "/tmp", true),
    ("/tmp", "..", false, "/", true),
    ("/tmp", "a/..", false, "/tmp", true),
    ("/tmp", "a/.", false, "/tmp/a", true),
    ("/tmp", "a/../..", false, "/", true),
    ("/tmp", "a/b/../c", false, "/tmp/a/c", false),
    ("/tmp", "a/../../b", false, "/b", false),
    ("/tmp", "a/../b/", false, "/tmp/b", true),
    ("/tmp/", "c", false, "/tmp/c", false),
    ("/tmp/", "c/../d", false, "/tmp/d", false),
    ("/home/user", "file", false, "/home/user/file", false),
    (
        "/home/user",
        "dir/file",
        false,
        "/home/user/dir/file",
        false,
    ),
    ("/home/user", "../other", false, "/home/other", false),
    ("/home/user", "../../x", false, "/x", false),
    ("/a/b", "c/../../d", false, "/a/d", false),
    ("/a/b", "../../../c", false, "/c", false),
    ("/a/b/c", "../../..", false, "/", true),
    ("/a/b/c", "../../../..", false, "/", true),
    ("/a/b/c", "d/../../e", false, "/a/b/e", false),
    ("/a/b/c", "./d/./e", false, "/a/b/c/d/e", false),
    ("/a/b/c", "d//e//f", false, "/a/b/c/d/e/f", false),
    ("/a", "b/c/d/e", false, "/a/b/c/d/e", false),
    ("/a", ".a", false, "/a/.a", false),
    ("/a", "a.", false, "/a/a.", false),
    ("/a", "-", false, "/a/-", false),
    ("/a", "_", false, "/a/_", false),
    ("/a", "~", false, "/a/~", false),
    ("/a", "a b", false, "/a/a b", false),
    ("/a", "a:b", false, "/a/a:b", false),
    ("/a", "a\\b", false, "/a/a\\b", false),
    ("/", "/a/./././b", false, "/a/b", false),
    ("/", "/a//./b//../c", false, "/a/c", false),
    ("/", "/./", false, "/", true),
    ("/", "/.", false, "/", true),
    ("/", "//", false, "/", true),
    ("/", "///", false, "/", true),
    ("/", "/a/..", false, "/", true),
    ("/", "/a/b/../../..", false, "/", true),
    ("/", "/a/b/c/../../d", false, "/a/d", false),
    ("/usr", "local/bin", false, "/usr/local/bin", false),
    ("/usr", "../lib", false, "/lib", false),
    ("/usr/local", "../../bin", false, "/bin", false),
    ("/usr/local", "./bin/", false, "/usr/local/bin", true),
    ("/usr/local", "bin/..", false, "/usr/local", true),
    ("/usr/local", "bin/.", false, "/usr/local/bin", true),
    ("/usr/local", "bin/../..", false, "/usr", true),
    ("/usr/local", "bin/../../..", false, "/", true),
    ("/a/b", "../c", false, "/a/c", false),
    ("/a/b", "./c/", false, "/a/b/c", true),
    ("/a/b", "c//", false, "/a/b/c", true),
    ("/a/b", "c/.//", false, "/a/b/c", true),
    ("/a/b", "c/..//", false, "/a/b", true),
    ("/a/b", "c/../d/", false, "/a/b/d", true),
    ("/a/b", "../../../../x", false, "/x", false),
    ("/a/b", "x/y/../../z", false, "/a/b/z", false),
    ("/", "a/b/c/d/e/f/g/h", false, "/a/b/c/d/e/f/g/h", false),
    (
        "/root",
        "a/b/c/d/e/f/g/h",
        false,
        "/root/a/b/c/d/e/f/g/h",
        false,
    ),
    ("/", "/a b/c d", false, "/a b/c d", false),
    ("/", "/a\tb", false, "/a\tb", false),
];

#[test]
fn resolver_table_has_at_least_eighty_cases() {
    assert!(OK.len() >= 80, "only {} cases", OK.len());
    let mut total = 0usize;
    for (base, path, empty, expected, must) in OK {
        let resolved = resolve(base.as_bytes(), path.as_bytes(), *empty, &DEFAULT_LIMITS)
            .unwrap_or_else(|error| panic!("{base} + {path}: {error:?}"));
        assert_eq!(resolved.abs, expected.as_bytes(), "{base} + {path}");
        assert_eq!(resolved.must_be_dir, *must, "{base} + {path}");
        total = total.saturating_add(1);
    }
    assert!(total >= 80);
}

#[test]
fn empty_path_is_enoent() {
    assert_eq!(
        resolve(b"/tmp", b"", false, &DEFAULT_LIMITS).unwrap_err(),
        PathReason::Empty
    );
}

#[test]
fn empty_path_with_at_empty_path_returns_base() {
    let resolved = resolve(b"/tmp", b"", true, &DEFAULT_LIMITS).expect("empty ok");
    assert_eq!(resolved.abs, b"/tmp");
    assert!(resolved.must_be_dir);
}

#[test]
fn dot_dot_past_root_is_clamped() {
    for path in [b"/..".as_slice(), b"/../../..", b"/a/../../.."] {
        let resolved = resolve(b"/", path, false, &DEFAULT_LIMITS).expect("valid");
        assert_eq!(resolved.abs, b"/", "{path:?}");
    }
}

#[test]
fn component_255_is_accepted() {
    let component = vec![b'a'; 255];
    let wide = Limits {
        path_max: 4096,
        component_max: 255,
        transport_max: 4096,
    };
    let resolved = resolve(b"/", &component, false, &wide).expect("255");
    assert_eq!(resolved.abs.len(), 256);
}

#[test]
fn component_256_is_enametoolong() {
    let component = vec![b'a'; 256];
    assert_eq!(
        resolve(b"/", &component, false, &DEFAULT_LIMITS).unwrap_err(),
        PathReason::ComponentTooLong
    );
}

#[test]
fn total_4095_is_rejected_by_the_transport_limit() {
    let mut path = Vec::new();
    for index in 0..16 {
        if index != 0 {
            path.push(b'/');
        }
        path.extend_from_slice(&[b'a'; 255]);
    }
    assert_eq!(path.len(), 4095);
    assert_eq!(
        resolve(b"/", &path, false, &DEFAULT_LIMITS).unwrap_err(),
        PathReason::TooLong
    );
}

#[test]
fn total_4096_is_enametoolong() {
    let mut path = Vec::new();
    for index in 0..17 {
        if index != 0 {
            path.push(b'/');
        }
        path.extend_from_slice(&[b'a'; 255]);
    }
    assert!(path.len() >= 4096);
    assert_eq!(
        resolve(b"/", &path, false, &DEFAULT_LIMITS).unwrap_err(),
        PathReason::TooLong
    );
}

#[test]
fn wide_limits_accept_4095() {
    let wide = Limits {
        path_max: 4096,
        component_max: 4095,
        transport_max: 4096,
    };
    let path = vec![b'a'; 4095];
    let resolved = resolve(b"/", &path, false, &wide).expect("4095");
    assert_eq!(resolved.abs.len(), 4096);
}

#[test]
fn nul_is_einval() {
    assert_eq!(
        resolve(b"/", b"a\0b", false, &DEFAULT_LIMITS).unwrap_err(),
        PathReason::Nul
    );
}

#[test]
fn relative_base_is_einval() {
    assert_eq!(
        resolve(b"tmp", b"a", false, &DEFAULT_LIMITS).unwrap_err(),
        PathReason::NotAbsolute
    );
}

#[test]
fn file_prefix_is_added_once() {
    assert_eq!(file_path(b"/etc/passwd"), b"file:/etc/passwd");
    assert_eq!(file_path(b"/"), b"file:/");
}

#[test]
fn dev_fd_names() {
    assert_eq!(special_fd(b"/dev/fd/3"), Some(SpecialName::Fd(3)));
    assert_eq!(special_fd(b"/proc/self/fd/7"), Some(SpecialName::Fd(7)));
    assert_eq!(special_fd(b"/dev/stdin"), Some(SpecialName::Stdin));
    assert_eq!(special_fd(b"/dev/stdout"), Some(SpecialName::Stdout));
    assert_eq!(special_fd(b"/dev/stderr"), Some(SpecialName::Stderr));
    assert_eq!(special_fd(b"/dev/fd/x"), None);
    assert_eq!(special_fd(b"/dev/null"), None);
    assert_eq!(target_fd(SpecialName::Fd(3)), 3);
    assert_eq!(target_fd(SpecialName::Stdin), 0);
    assert_eq!(target_fd(SpecialName::Stdout), 1);
    assert_eq!(target_fd(SpecialName::Stderr), 2);
}
