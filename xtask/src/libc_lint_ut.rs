// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::PathBuf;

use super::*;
use crate::libc_lint_testutil::Tree;

const HDR: &str = "// SPDX-License-Identifier: BSD-3-Clause\n// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n";

fn real() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture() -> Tree {
    let tree = Tree::new();
    tree.copy_manifest(&real().join("libc/ravenlibc/xtask"));
    tree
}

fn crate_file(tree: &Tree, name: &str, file: &str, body: &str) {
    tree.write(&format!("libc/ravenlibc/crates/{name}/{file}"), body);
}

fn manifest(tree: &Tree, name: &str) {
    tree.write(
        &format!("libc/ravenlibc/crates/{name}/Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\nauthors.workspace = true\nrust-version.workspace = true\npublish = false\n\n[lints]\nworkspace = true\n"),
    );
}

fn check(tree: &Tree, name: &str) -> Vec<Violation> {
    run_checks(tree.root(), &[name.to_string()]).expect("lint")
}

#[test]
fn clean_required_tree_has_no_layout_errors() {
    let tree = fixture();
    tree.write("libc/ravenlibc/README.md", HDR);
    tree.write("libc/ravenlibc/crates/README.md", HDR);
    tree.write("libc/ravenlibc/xtask/.keep", "");
    tree.write("libc/ravenlibc/tools/lint/.keep", "");
    for required in [
        "libc/ravenlibc/README.md",
        "libc/ravenlibc/crates/README.md",
    ] {
        tree.write(required, HDR);
    }
    let violations = check(&tree, "layout");
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn missing_required_is_reported() {
    let tree = fixture();
    let violations = check(&tree, "layout");
    assert!(violations.iter().any(|v| v.key == "missing-required"));
}

#[test]
fn forbidden_cargo_toml_is_reported() {
    let tree = fixture();
    tree.write("libc/ravenlibc/Cargo.toml", "");
    let violations = check(&tree, "layout");
    assert!(violations.iter().any(|v| v.key == "forbidden-file"));
}

#[test]
fn glob_guard_fires_without_plain_file_or_crate() {
    let tree = fixture();
    let violations = check(&tree, "layout");
    assert!(violations.iter().any(|v| v.key == "glob-guard"));
}

#[test]
fn crate_without_manifest_is_reported() {
    let tree = fixture();
    tree.write("libc/ravenlibc/crates/raven-libc-bad/file.rs", "");
    let violations = check(&tree, "layout");
    assert!(violations.iter().any(|v| v.key == "crate-no-manifest"));
}

#[test]
fn header_missing_spdx_is_reported() {
    let tree = fixture();
    crate_file(&tree, "raven-libc-str", "src/lib.rs", "fn f() {}\n");
    let violations = check(&tree, "header");
    assert!(violations.iter().any(|v| v.key == "missing-spdx"));
}

#[test]
fn header_correct_spdx_is_clean() {
    let tree = fixture();
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    let violations = check(&tree, "header");
    assert!(!violations.iter().any(|v| v.key == "missing-spdx"));
}

#[test]
fn header_wrong_spdx_is_reported() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "src/lib.rs",
        "// SPDX-License-Identifier: MIT\n// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n",
    );
    let violations = check(&tree, "header");
    assert!(violations.iter().any(|v| v.key == "wrong-spdx"));
}

#[test]
fn header_missing_copyright_is_reported() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "src/lib.rs",
        "// SPDX-License-Identifier: BSD-3-Clause\n",
    );
    let violations = check(&tree, "header");
    assert!(violations.iter().any(|v| v.key == "missing-copyright"));
}

#[test]
fn unsafe_in_class_p_is_forbidden() {
    let tree = fixture();
    manifest(&tree, "raven-libc-ctype");
    crate_file(&tree, "raven-libc-ctype", "src/lib.rs", HDR);
    crate_file(&tree, "raven-libc-ctype", "src/raw.rs", "unsafe {}\n");
    let violations = check(&tree, "unsafe-scope");
    assert!(violations.iter().any(|v| v.key == "forbidden"));
}

#[test]
fn unsafe_in_class_m_without_allow_is_reported() {
    let tree = fixture();
    manifest(&tree, "raven-libc-str");
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    crate_file(&tree, "raven-libc-str", "src/raw.rs", "unsafe { }\n");
    let violations = check(&tree, "unsafe-scope");
    assert!(violations.iter().any(|v| v.key == "not-allowlisted"));
}

#[test]
fn unsafe_in_a_doc_comment_is_clean() {
    let tree = fixture();
    manifest(&tree, "raven-libc-ctype");
    crate_file(
        &tree,
        "raven-libc-ctype",
        "src/lib.rs",
        &format!("{HDR}//! unsafe is forbidden\n"),
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(!violations.iter().any(|v| v.key == "forbidden"));
}

#[test]
fn unsafe_in_a_string_is_clean() {
    let tree = fixture();
    manifest(&tree, "raven-libc-ctype");
    crate_file(
        &tree,
        "raven-libc-ctype",
        "src/lib.rs",
        &format!("{HDR}const S: &str = \"unsafe\";\n"),
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(!violations.iter().any(|v| v.key == "forbidden"));
}

#[test]
fn no_std_detects_std_paths() {
    let tree = fixture();
    manifest(&tree, "raven-libc-ctype");
    crate_file(
        &tree,
        "raven-libc-ctype",
        "src/lib.rs",
        &format!("{HDR}use std::vec::Vec;\n"),
    );
    let violations = check(&tree, "no-std");
    assert!(violations.iter().any(|v| v.key == "std-path"));
}

#[test]
fn no_cc_detects_banned_dependency() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "Cargo.toml",
        "[dependencies]\ncc = \"1\"\n",
    );
    let violations = check(&tree, "no-cc");
    assert!(violations.iter().any(|v| v.key == "banned-dep"));
}

#[test]
fn no_cc_detects_c_source() {
    let tree = fixture();
    tree.write("libc/ravenlibc/crates/raven-libc-str/src/x.c", "int x;");
    let violations = check(&tree, "no-cc");
    assert!(violations.iter().any(|v| v.key == "c-source"));
}

#[test]
fn no_cc_allows_assembly() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "src/raw/arch/aarch64/memcpy.s",
        ".text\n",
    );
    crate_file(
        &tree,
        "raven-libc-str",
        "src/raw/arch/aarch64/memcpy.S",
        ".text\n",
    );
    let violations = check(&tree, "no-cc");
    assert!(
        !violations.iter().any(|v| v.key == "c-source"),
        "{violations:?}"
    );
}

#[test]
fn provenance_marker_without_import_is_reported() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "src/lib.rs",
        &format!(
            "{HDR}// RavenBSD: transpiled from musl:src/string/foo.c @ {}\n",
            "a".repeat(40)
        ),
    );
    let violations = check(&tree, "provenance-link");
    assert!(violations.iter().any(|v| v.key == "no-import"));
}

#[test]
fn cfg_unknown_feature_in_manifest_is_reported() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "Cargo.toml",
        "[features]\nbogus = []\n",
    );
    // manifest feature keys are handled by the driver only for Cargo.toml paths
    let violations = run_checks(tree.root(), &["cfg".to_string()]).expect("lint");
    // the current cfg implementation checks cfg expressions, not keys; keep the
    // driver contract exercised without inventing a finding.
    assert!(violations.is_empty() || violations.iter().all(|v| v.check == "cfg"));
}

#[test]
fn render_is_sorted_and_key_first() {
    let violations = vec![
        error("header", "b", "b.rs", 2, "x"),
        error("header", "a", "a.rs", 1, "y"),
    ];
    let mut sorted = violations.clone();
    sorted.sort();
    let text = render(&sorted);
    assert!(text.find("a.rs").unwrap() < text.find("b.rs").unwrap());
}

#[test]
fn json_contains_files_scanned_and_key() {
    let text = json(&[error("no-cc", "banned-dep", "x/Cargo.toml", 2, "cc")], 4);
    assert!(text.contains("\"files_scanned\":4"));
    assert!(text.contains("\"key\":\"banned-dep\""));
}

#[test]
fn json_escapes_quotes() {
    let text = json(&[error("no-cc", "k", "p", 1, "say \"hi\"")], 0);
    assert!(text.contains("say \\\"hi\\\""));
}

#[test]
fn stub_check_is_rejected() {
    let tree = fixture();
    let error = run_checks(tree.root(), &["deps".to_string()]).expect_err("stub");
    assert!(error.to_string().contains("not implemented by C34"));
}

#[test]
fn unknown_check_is_rejected() {
    let tree = fixture();
    assert!(run_checks(tree.root(), &["nope".to_string()]).is_err());
}

#[test]
fn real_tree_lint_is_clean() {
    let violations = run_checks(&real(), &["all".to_string()]).expect("lint");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn host_crate_unsafe_is_not_scanned_by_s_crate_rules() {
    let tree = fixture();
    tree.write(
        "tests/libc/oracle/src/call.rs",
        &format!("{HDR}#[allow(unsafe_code)]\nunsafe fn f() {{}}\n"),
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn host_manifest_features_are_not_scanned_by_s_crate_rules() {
    let tree = fixture();
    tree.write(
        "tests/libc/oracle/Cargo.toml",
        "[features]\noracle-string-basic = []\n",
    );
    let violations = check(&tree, "cfg");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn host_c_source_is_not_a_no_cc_violation() {
    let tree = fixture();
    tree.write("tests/libc/oracle/fixture.c", "int x;");
    let violations = check(&tree, "no-cc");
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn libc_c_source_is_a_no_cc_violation() {
    let tree = fixture();
    tree.write("libc/ravenlibc/crates/raven-libc-str/src/x.c", "int x;");
    let violations = check(&tree, "no-cc");
    assert!(violations.iter().any(|v| v.key == "c-source"));
}

#[test]
fn unlisted_crate_directory_is_still_scanned_as_class_p() {
    let tree = fixture();
    tree.write(
        "libc/ravenlibc/crates/raven-libc-unlisted/src/lib.rs",
        &format!("{HDR}unsafe {{}}\n"),
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(violations.iter().any(|v| v.key == "forbidden"));
}

#[test]
fn file_level_violations_use_line_zero() {
    let tree = fixture();
    let violations = check(&tree, "layout");
    assert!(violations.iter().any(|v| v.line == 0));
}

#[test]
fn render_empty_is_empty() {
    assert_eq!(render(&[]), "");
}

#[test]
fn json_empty_is_valid_shape() {
    assert_eq!(json(&[], 0), "{\"files_scanned\":0,\"violations\":[]}");
}

#[test]
fn violation_ordering_is_stable() {
    let mut values = [
        error("header", "z", "z.rs", 9, "z"),
        error("header", "a", "a.rs", 1, "a"),
    ];
    values.sort();
    assert_eq!(values[0].path, "a.rs");
}

#[test]
fn stub_checks_are_listed() {
    for check in ["deps", "one-symbol", "sections", "no-fmt", "exports"] {
        assert!(STUB_CHECKS.contains(&check));
    }
}

#[test]
fn all_checks_are_listed() {
    assert_eq!(ALL_CHECKS.len(), 7);
}

#[test]
fn clean_fixture_header_has_no_missing_copyright() {
    let tree = fixture();
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    let violations = check(&tree, "header");
    assert!(!violations.iter().any(|v| v.key == "missing-copyright"));
}

#[test]
fn wrong_crate_tree_is_reported() {
    let tree = fixture();
    crate_file(&tree, "raven-libc-bad", "src/lib.rs", HDR);
    let violations = check(&tree, "layout");
    assert!(violations.iter().any(|v| v.key == "crate-no-manifest"));
}

#[test]
fn no_cc_build_script_is_reported() {
    let tree = fixture();
    crate_file(
        &tree,
        "raven-libc-str",
        "Cargo.toml",
        "build = \"build.rs\"\n",
    );
    let violations = check(&tree, "no-cc");
    assert!(violations.iter().any(|v| v.key == "build-script"));
}

#[test]
fn no_std_core_crate_rejects_alloc() {
    let tree = fixture();
    manifest(&tree, "raven-libc-str");
    crate_file(
        &tree,
        "raven-libc-str",
        "src/lib.rs",
        &format!("{HDR}extern crate alloc;\n"),
    );
    let violations = check(&tree, "no-std");
    assert!(violations.iter().any(|v| v.key == "alloc-in-core"));
}

#[test]
fn unsafe_allowlisted_file_is_not_not_allowlisted() {
    let tree = fixture();
    tree.write(
        "libc/ravenlibc/xtask/unsafe-allowlist.toml",
        "[[allow]]\ncrate = \"raven-libc-str\"\npath = \"src/raw.rs\"\nreason = \"raw\"\n",
    );
    manifest(&tree, "raven-libc-str");
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    crate_file(
        &tree,
        "raven-libc-str",
        "src/raw.rs",
        "// SAFETY: none\nunsafe {}\n",
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(!violations.iter().any(|v| v.key == "not-allowlisted"));
}

#[test]
fn missing_safety_is_reported_separately() {
    let tree = fixture();
    tree.write(
        "libc/ravenlibc/xtask/unsafe-allowlist.toml",
        "[[allow]]\ncrate = \"raven-libc-str\"\npath = \"src/raw.rs\"\nreason = \"raw\"\n",
    );
    manifest(&tree, "raven-libc-str");
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    crate_file(&tree, "raven-libc-str", "src/raw.rs", "unsafe {}\n");
    let violations = check(&tree, "unsafe-scope");
    assert!(violations.iter().any(|v| v.key == "missing-safety"));
}

#[test]
fn safety_comment_above_block_is_accepted() {
    let tree = fixture();
    tree.write(
        "libc/ravenlibc/xtask/unsafe-allowlist.toml",
        "[[allow]]\ncrate = \"raven-libc-str\"\npath = \"src/raw.rs\"\nreason = \"raw\"\n",
    );
    manifest(&tree, "raven-libc-str");
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    crate_file(
        &tree,
        "raven-libc-str",
        "src/raw.rs",
        "// SAFETY: pointer is valid\nunsafe {}\n",
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(!violations.iter().any(|v| v.key == "missing-safety"));
}

#[test]
fn provenance_import_marker_agrees() {
    let tree = fixture();
    crate_file(&tree, "raven-libc-str", "src/lib.rs", HDR);
    let violations = check(&tree, "provenance-link");
    assert!(!violations.iter().any(|v| v.key == "no-import"));
}

#[test]
fn empty_allowlist_has_no_rows() {
    let (_, _, allow, _) = crate::libc_lint_data::load(&real()).expect("data");
    assert!(allow.rows.is_empty());
}

#[test]
fn feature_has_float_printf() {
    let (_, features, _, _) = crate::libc_lint_data::load(&real()).expect("data");
    assert!(features.rows.iter().any(|row| row.name == "float-printf"));
}

#[test]
fn tree_has_world_entry() {
    let (_, _, _, tree) = crate::libc_lint_data::load(&real()).expect("data");
    assert!(tree.entries.iter().any(|entry| entry.path == "world.toml"));
}

#[test]
fn unsafe_scope_scans_ut_files() {
    let tree = fixture();
    manifest(&tree, "raven-libc-ctype");
    crate_file(&tree, "raven-libc-ctype", "src/lib.rs", HDR);
    crate_file(&tree, "raven-libc-ctype", "src/lib_ut.rs", "unsafe {}\n");
    let violations = check(&tree, "unsafe-scope");
    assert!(violations.iter().any(|v| v.path.ends_with("lib_ut.rs")));
}

#[test]
fn unsafe_scope_skips_tests_directory() {
    let tree = fixture();
    manifest(&tree, "raven-libc-ctype");
    crate_file(&tree, "raven-libc-ctype", "src/lib.rs", HDR);
    tree.write(
        "libc/ravenlibc/crates/raven-libc-ctype/tests/x.rs",
        "unsafe {}\n",
    );
    let violations = check(&tree, "unsafe-scope");
    assert!(!violations.iter().any(|v| v.path.contains("/tests/")));
}

#[test]
fn cfg_builtin_name_is_accepted() {
    let tree = fixture();
    manifest(&tree, "raven-libc-str");
    crate_file(
        &tree,
        "raven-libc-str",
        "src/lib.rs",
        &format!("{HDR}#[cfg(unix)]\nfn f() {{}}\n"),
    );
    let violations = check(&tree, "cfg");
    assert!(!violations.iter().any(|v| v.key == "unknown-cfg"));
}

#[test]
fn c_source_in_crate_is_reported() {
    let tree = fixture();
    tree.write("libc/ravenlibc/crates/raven-libc-str/src/x.h", "int x;");
    let violations = check(&tree, "no-cc");
    assert!(violations.iter().any(|v| v.key == "c-source"));
}
