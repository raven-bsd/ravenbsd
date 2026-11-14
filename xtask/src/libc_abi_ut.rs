// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const CENSUS: &str = r#"
complete = true
census_rev = "r1"
[[symbol]]
name = "strlen"
ring = 0
header = "string.h"
kind = "func"
[[symbol]]
name = "strlcpy"
ring = 1
header = "string.h"
kind = "func"
[[symbol]]
name = "environ"
ring = 0
header = "unistd.h"
kind = "data"
[[symbol]]
name = "regcomp"
ring = 0
header = "regex.h"
kind = "func"
"#;

const MAP: &str = r#"
RAVEN_1.0 {
    global:
        strlen;
        strlcpy;
        environ;
        __environ;
};
RAVENprivate_1.0 {
    global:
        __raven_libc_start;
};
"#;

const DATA: &str = r#"
[[data]]
name = "environ"
size = 8
type = "char **"
aliases = ["__environ"]
source = "doc 02 2.2"
verified = false
"#;

fn model() -> Model {
    let census = parse_census(CENSUS).expect("census");
    let exports = parse_symbol_map(MAP).expect("map");
    let data = parse_data_symbols(DATA).expect("data");
    derive(&census, &exports, &data).expect("derive")
}

#[test]
fn the_shell_map_parses_blocks_and_scope() {
    let exports = parse_symbol_map(MAP).expect("map");
    assert_eq!(exports.len(), 5);
    assert_eq!(exports[0].node, "RAVEN_1.0");
    assert_eq!(exports[3].name, "__environ");
    assert_eq!(exports[4].node, "RAVENprivate_1.0");
}

#[test]
fn the_map_rejects_a_missing_semicolon() {
    let error = parse_symbol_map("RAVEN_1.0 {\n strlen\n};\n").expect_err("error");
    assert!(error.to_string().contains("missing"));
}

#[test]
fn the_map_keeps_an_arch_comment() {
    let exports = parse_symbol_map("RAVEN_1.0 {\n f; # x86_64 aarch64\n};").expect("map");
    assert_eq!(exports[0].arches, "x86_64,aarch64");
}

#[test]
fn local_star_is_ignored() {
    let exports = parse_symbol_map("RAVEN_1.0 {\n local:\n *;\n global:\n f;\n};").expect("map");
    assert_eq!(exports.len(), 1);
}

#[test]
fn the_census_reads_ring_and_kind() {
    let census = parse_census(CENSUS).expect("census");
    assert!(census.complete);
    assert_eq!(census.rows["strlen"].ring, 0);
    assert_eq!(census.rows["environ"].kind, "data");
}

#[test]
fn an_incomplete_census_is_still_usable_for_gap_triage() {
    let census = parse_census("complete = false\n").expect("census");
    let model = derive(&census, &[], &BTreeMap::new()).expect("derive");
    assert!(model.public.is_empty());
}

#[test]
fn gen_refuses_an_incomplete_census() {
    let root = fixture_tree();
    fs::write(
        root.join("libc/ravenlibc/abi/census/symbols.toml"),
        "complete = false\n",
    )
    .expect("census");
    assert_eq!(
        run(&root, &["gen".to_string(), "--check".to_string()]).expect("run"),
        2
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn public_and_private_sets_are_split() {
    let model = model();
    assert!(model.public.contains(&"strlen".to_string()));
    assert!(model.public.contains(&"__environ".to_string()));
    assert!(model.private.contains(&"__raven_libc_start".to_string()));
    assert!(!model.public.contains(&"__raven_libc_start".to_string()));
}

#[test]
fn a_data_alias_inherits_its_size() {
    let model = model();
    assert!(
        model
            .records
            .iter()
            .any(|row| row == "__environ D 8 all 0 unistd.h")
    );
}

#[test]
fn a_census_only_name_is_a_gap() {
    let model = model();
    assert_eq!(model.census_only, vec!["regcomp"]);
}

#[test]
fn an_export_only_name_is_reported() {
    let census = parse_census(CENSUS).expect("census");
    let exports = parse_symbol_map("RAVEN_1.0 {\n newsym;\n};").expect("map");
    let model = derive(&census, &exports, &BTreeMap::new()).expect("derive");
    assert_eq!(model.export_only, vec!["newsym"]);
}

#[test]
fn the_private_prefix_is_not_public() {
    let census = parse_census(CENSUS).expect("census");
    let exports = parse_symbol_map("RAVEN_1.0 {\n __raven_x;\n};").expect("map");
    let model = derive(&census, &exports, &BTreeMap::new()).expect("derive");
    assert!(model.private.contains(&"__raven_x".to_string()));
    assert!(model.public.is_empty());
}

#[test]
fn the_rendered_lists_are_sorted_and_split() {
    let census = parse_census(CENSUS).expect("census");
    let exports = parse_symbol_map(MAP).expect("map");
    let data = parse_data_symbols(DATA).expect("data");
    let model = derive(&census, &exports, &data).expect("derive");
    let rendered = render(&model, &census);
    let public = rendered
        .symbols
        .lines()
        .filter(|line| !line.starts_with('#') && !line.starts_with("/*") && !line.starts_with(" *"))
        .collect::<Vec<_>>();
    assert_eq!(public, vec!["__environ", "environ", "strlcpy", "strlen"]);
    assert!(rendered.private.contains("__raven_libc_start"));
    assert!(rendered.frozen.contains("# status: candidate"));
    assert!(rendered.frozen.contains("strlen F - all 0 string.h"));
}

#[test]
fn the_versions_chain_has_the_private_node_last() {
    let census = parse_census(CENSUS).expect("census");
    let rendered = render(&model(), &census);
    let private = rendered
        .versions
        .rfind("RAVENprivate_1.0")
        .expect("private");
    let public = rendered.versions.find("RAVEN_1.0").expect("public");
    assert!(private > public);
    assert!(rendered.versions.contains("} RAVEN_1.0;"));
}

#[test]
fn a_missing_data_size_is_an_error() {
    let census = parse_census(CENSUS).expect("census");
    let exports = parse_symbol_map("RAVEN_1.0 {\n environ;\n};").expect("map");
    let error = derive(&census, &exports, &BTreeMap::new()).expect_err("size");
    assert!(error.to_string().contains("data-size-missing"));
}

fn fixture_tree() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("c68-abi-{}-{serial}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("libc/ravenlibc/abi/census")).expect("census dir");
    fs::create_dir_all(root.join("libc/ravenlibc/crates/raven-libc")).expect("shell dir");
    fs::write(root.join("libc/ravenlibc/abi/census/symbols.toml"), CENSUS).expect("census");
    fs::write(root.join("libc/ravenlibc/abi/data-symbols.toml"), DATA).expect("data");
    fs::write(
        root.join("libc/ravenlibc/crates/raven-libc/Symbol.map"),
        MAP,
    )
    .expect("map");
    root
}

#[test]
fn gen_writes_the_four_files_and_check_is_clean() {
    let root = fixture_tree();
    assert_eq!(run(&root, &["gen".to_string()]).expect("gen"), 0);
    assert_eq!(
        run(&root, &["gen".to_string(), "--check".to_string()]).expect("check"),
        0
    );
    assert!(
        root.join("libc/ravenlibc/abi/frozen/RAVEN_1.0.symbols")
            .is_file()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn check_detects_a_changed_symbols_list() {
    let root = fixture_tree();
    assert_eq!(run(&root, &["gen".to_string()]).expect("gen"), 0);
    fs::write(root.join("libc/ravenlibc/abi/Symbols.list"), "# changed\n").expect("write");
    assert_eq!(
        run(&root, &["gen".to_string(), "--check".to_string()]).expect("check"),
        1
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn gaps_and_reconcile_write_markdown() {
    let root = fixture_tree();
    assert_eq!(run(&root, &["gaps".to_string()]).expect("gaps"), 0);
    assert_eq!(
        run(&root, &["reconcile".to_string()]).expect("reconcile"),
        0
    );
    assert!(root.join("libc/ravenlibc/abi/GAPS.md").is_file());
    assert!(root.join("libc/ravenlibc/abi/RECONCILE.md").is_file());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_gaps_markdown_lists_census_only_rows() {
    let census = parse_census(CENSUS).expect("census");
    let markdown = gaps_markdown(&census, &model());
    assert!(markdown.contains("| regcomp | 0 | regex.h |"));
}

#[test]
fn the_reconcile_markdown_has_totals() {
    let markdown = reconcile_markdown(&model());
    assert!(markdown.contains("census_only: 1"));
}

#[test]
fn the_ring_three_stub_is_frozen() {
    let census_text = CENSUS.replace(
        "[[symbol]]\nname = \"regcomp\"",
        "[[symbol]]\nname = \"gets\"\nring = 3\nheader = \"stdio.h\"\nkind = \"func\"\nstub = \"reads\"\n[[symbol]]\nname = \"regcomp\"",
    );
    let census = parse_census(&census_text).expect("census");
    let exports = parse_symbol_map("RAVEN_1.0 {\n gets;\n};").expect("map");
    let model = derive(&census, &exports, &BTreeMap::new()).expect("derive");
    assert!(
        model
            .records
            .iter()
            .any(|row| row.starts_with("gets F - all 3"))
    );
}
