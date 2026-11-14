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
        .split_once("\n# private\n")
        .map_or("", |(public, _)| public)
        .lines()
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with("/*")
                && !line.starts_with(" *")
        })
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

const FROZEN: &str = "# node: RAVEN_1.0\n# status: candidate\n# records: 2\nstrlen F - all 0 string.h\nenviron D 8 all 0 unistd.h\n";

#[test]
fn audit_accepts_an_identical_frozen_file() {
    assert!(audit_frozen(FROZEN, FROZEN).is_empty());
}

#[test]
fn audit_reports_a_removed_frozen_symbol() {
    let actual = FROZEN.replace("strlen F - all 0 string.h\n", "");
    assert!(
        audit_frozen(FROZEN, &actual)
            .iter()
            .any(|(key, _)| key == "frozen-missing")
    );
}

#[test]
fn audit_reports_a_changed_kind() {
    let actual = FROZEN.replace("strlen F", "strlen D");
    assert!(
        audit_frozen(FROZEN, &actual)
            .iter()
            .any(|(key, _)| key == "frozen-kind")
    );
}

#[test]
fn audit_reports_a_changed_size() {
    let actual = FROZEN.replace("environ D 8", "environ D 16");
    assert!(
        audit_frozen(FROZEN, &actual)
            .iter()
            .any(|(key, _)| key == "frozen-size")
    );
}

#[test]
fn audit_reports_a_narrowed_arch_list() {
    let expected = FROZEN.replace("strlen F - all", "strlen F - x86_64,aarch64");
    let actual = FROZEN.replace("strlen F - all", "strlen F - x86_64");
    assert!(
        audit_frozen(&expected, &actual)
            .iter()
            .any(|(key, _)| key == "frozen-arch")
    );
}

#[test]
fn audit_reports_a_changed_ring() {
    let actual = FROZEN.replace("strlen F - all 0", "strlen F - all 1");
    assert!(
        audit_frozen(FROZEN, &actual)
            .iter()
            .any(|(key, _)| key == "frozen-ring")
    );
}

#[test]
fn audit_reports_a_new_name_in_the_frozen_node() {
    let actual = format!("{FROZEN}newsym F - all 0 string.h\n");
    assert!(
        audit_frozen(FROZEN, &actual)
            .iter()
            .any(|(key, _)| key == "frozen-new")
    );
}

#[test]
fn a_bad_shell_map_is_a_parse_error() {
    let error = parse_symbol_map("RAVEN_1.0 {\n f\n").expect_err("map");
    assert!(error.to_string().contains("missing"));
}

#[test]
fn a_bad_census_is_not_complete() {
    let census = parse_census("complete = false\n").expect("census");
    assert!(!census.complete);
}

#[test]
fn a_bad_data_size_is_reported() {
    let census = parse_census(CENSUS).expect("census");
    let exports = parse_symbol_map("RAVEN_1.0 {\n environ;\n};").expect("map");
    let error = derive(&census, &exports, &BTreeMap::new()).expect_err("data");
    assert!(error.to_string().contains("data-size-missing"));
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

const CUSTOM: &str = "# node: RAVEN_1.0\n# status: candidate\n# records: 4\nfoo F - all 0 x.h\nbar D 4 all 1 y.h\nbaz F - x86_64 2 z.h\nqux D 8 all 3 h.h\n";

macro_rules! audit_case {
    ($name:ident, $expected:expr, $actual:expr, $key:expr) => {
        #[test]
        fn $name() {
            let findings = audit_frozen(&$expected, &$actual);
            assert!(
                findings.iter().any(|(key, _)| key == $key),
                "expected {} in {findings:?}",
                $key
            );
        }
    };
}

audit_case!(
    custom_missing_foo,
    CUSTOM,
    CUSTOM.replace("foo F - all 0 x.h\n", ""),
    "frozen-missing"
);
audit_case!(
    custom_missing_bar,
    CUSTOM,
    CUSTOM.replace("bar D 4 all 1 y.h\n", ""),
    "frozen-missing"
);
audit_case!(
    custom_missing_baz,
    CUSTOM,
    CUSTOM.replace("baz F - x86_64 2 z.h\n", ""),
    "frozen-missing"
);
audit_case!(
    custom_missing_qux,
    CUSTOM,
    CUSTOM.replace("qux D 8 all 3 h.h\n", ""),
    "frozen-missing"
);
audit_case!(
    custom_kind_foo,
    CUSTOM,
    CUSTOM.replace("foo F", "foo D"),
    "frozen-kind"
);
audit_case!(
    custom_kind_bar,
    CUSTOM,
    CUSTOM.replace("bar D", "bar F"),
    "frozen-kind"
);
audit_case!(
    custom_kind_baz,
    CUSTOM,
    CUSTOM.replace("baz F", "baz D"),
    "frozen-kind"
);
audit_case!(
    custom_kind_qux,
    CUSTOM,
    CUSTOM.replace("qux D", "qux F"),
    "frozen-kind"
);
audit_case!(
    custom_size_bar,
    CUSTOM,
    CUSTOM.replace("bar D 4", "bar D 8"),
    "frozen-size"
);
audit_case!(
    custom_size_qux,
    CUSTOM,
    CUSTOM.replace("qux D 8", "qux D 16"),
    "frozen-size"
);
audit_case!(
    custom_size_bar_small,
    CUSTOM,
    CUSTOM.replace("bar D 4", "bar D 2"),
    "frozen-size"
);
audit_case!(
    custom_size_qux_small,
    CUSTOM,
    CUSTOM.replace("qux D 8", "qux D 4"),
    "frozen-size"
);
audit_case!(
    custom_arch_foo,
    CUSTOM,
    CUSTOM.replace("foo F - all", "foo F - x86_64"),
    "frozen-arch"
);
audit_case!(
    custom_arch_bar,
    CUSTOM,
    CUSTOM.replace("bar D 4 all", "bar D 4 aarch64"),
    "frozen-arch"
);
audit_case!(
    custom_arch_baz,
    CUSTOM,
    CUSTOM.replace("baz F - x86_64", "baz F - riscv64"),
    "frozen-arch"
);
audit_case!(
    custom_arch_qux,
    CUSTOM,
    CUSTOM.replace("qux D 8 all", "qux D 8 x86_64"),
    "frozen-arch"
);
audit_case!(
    custom_ring_foo,
    CUSTOM,
    CUSTOM.replace("foo F - all 0", "foo F - all 1"),
    "frozen-ring"
);
audit_case!(
    custom_ring_bar,
    CUSTOM,
    CUSTOM.replace("bar D 4 all 1", "bar D 4 all 2"),
    "frozen-ring"
);
audit_case!(
    custom_ring_baz,
    CUSTOM,
    CUSTOM.replace("baz F - x86_64 2", "baz F - x86_64 3"),
    "frozen-ring"
);
audit_case!(
    custom_ring_qux,
    CUSTOM,
    CUSTOM.replace("qux D 8 all 3", "qux D 8 all 2"),
    "frozen-ring"
);
audit_case!(
    custom_header_foo,
    CUSTOM,
    CUSTOM.replace("x.h", "a.h"),
    "frozen-header"
);
audit_case!(
    custom_header_bar,
    CUSTOM,
    CUSTOM.replace("y.h", "b.h"),
    "frozen-header"
);
audit_case!(
    custom_header_baz,
    CUSTOM,
    CUSTOM.replace("z.h", "c.h"),
    "frozen-header"
);
audit_case!(
    custom_header_qux,
    CUSTOM,
    CUSTOM.replace("h.h", "d.h"),
    "frozen-header"
);
audit_case!(
    custom_new_foo,
    CUSTOM,
    format!("{CUSTOM}new1 F - all 0 x.h\n"),
    "frozen-new"
);
audit_case!(
    custom_new_bar,
    CUSTOM,
    format!("{CUSTOM}new2 D 4 all 0 x.h\n"),
    "frozen-new"
);
audit_case!(
    custom_new_baz,
    CUSTOM,
    format!("{CUSTOM}new3 F - all 0 x.h\n"),
    "frozen-new"
);
audit_case!(
    custom_new_qux,
    CUSTOM,
    format!("{CUSTOM}new4 F - all 0 x.h\n"),
    "frozen-new"
);
audit_case!(
    custom_version_1,
    CUSTOM,
    CUSTOM.replace("# node: RAVEN_1.0", "# node: RAVEN_1.1"),
    "frozen-version"
);
audit_case!(
    custom_version_2,
    CUSTOM,
    CUSTOM.replace("# node: RAVEN_1.0", "# node: RAVEN_1.2"),
    "frozen-version"
);
audit_case!(
    custom_version_3,
    CUSTOM,
    CUSTOM.replace("# node: RAVEN_1.0", "# node: RAVEN_2.0"),
    "frozen-version"
);
audit_case!(
    custom_version_4,
    CUSTOM,
    CUSTOM.replace("# node: RAVEN_1.0", "# node: RAVEN_1.3"),
    "frozen-version"
);

#[test]
fn txt_and_symbols_accept_the_same_name_set() {
    assert!(txt_matches_symbols(
        "strlen F - all 0 string.h\nenviron D 8 all 0 unistd.h\n",
        "strlen F 7 x86_64\nenviron D 8 x86_64\n"
    ));
}

#[test]
fn txt_and_symbols_reject_a_divergent_name_set() {
    assert!(!txt_matches_symbols(
        "strlen F - all 0 string.h\nenviron D 8 all 0 unistd.h\n",
        "strlen F 7 x86_64\n"
    ));
}

#[test]
fn txt_and_symbols_reject_an_extra_txt_name() {
    assert!(!txt_matches_symbols(
        "strlen F - all 0 string.h\n",
        "strlen F 7 x86_64\nenviron D 8 x86_64\n"
    ));
}

#[test]
fn frozen_pass_custom_is_clean() {
    assert!(audit_frozen(CUSTOM, CUSTOM).is_empty());
}

#[test]
fn frozen_pass_default_is_clean() {
    assert!(audit_frozen(FROZEN, FROZEN).is_empty());
}

#[test]
fn frozen_pass_default_no_extra() {
    assert!(
        !audit_frozen(FROZEN, FROZEN)
            .iter()
            .any(|(key, _)| key == "frozen-new")
    );
}

#[test]
fn frozen_pass_custom_no_version() {
    assert!(
        !audit_frozen(CUSTOM, CUSTOM)
            .iter()
            .any(|(key, _)| key == "frozen-version")
    );
}

audit_case!(
    extra_missing_a,
    CUSTOM,
    CUSTOM.replace("foo F - all 0 x.h\n", ""),
    "frozen-missing"
);
audit_case!(
    extra_missing_b,
    CUSTOM,
    CUSTOM.replace("bar D 4 all 1 y.h\n", ""),
    "frozen-missing"
);
audit_case!(
    extra_missing_c,
    CUSTOM,
    CUSTOM.replace("baz F - x86_64 2 z.h\n", ""),
    "frozen-missing"
);
audit_case!(
    extra_missing_d,
    CUSTOM,
    CUSTOM.replace("qux D 8 all 3 h.h\n", ""),
    "frozen-missing"
);
audit_case!(
    extra_kind_a,
    CUSTOM,
    CUSTOM.replace("foo F", "foo D"),
    "frozen-kind"
);
audit_case!(
    extra_kind_b,
    CUSTOM,
    CUSTOM.replace("bar D", "bar F"),
    "frozen-kind"
);
audit_case!(
    extra_size_a,
    CUSTOM,
    CUSTOM.replace("bar D 4", "bar D 6"),
    "frozen-size"
);
audit_case!(
    extra_size_b,
    CUSTOM,
    CUSTOM.replace("qux D 8", "qux D 12"),
    "frozen-size"
);
audit_case!(
    extra_arch_a,
    CUSTOM,
    CUSTOM.replace("baz F - x86_64", "baz F - aarch64"),
    "frozen-arch"
);
audit_case!(
    extra_arch_b,
    CUSTOM,
    CUSTOM.replace("qux D 8 all", "qux D 8 riscv64"),
    "frozen-arch"
);
audit_case!(
    extra_ring_a,
    CUSTOM,
    CUSTOM.replace("foo F - all 0", "foo F - all 3"),
    "frozen-ring"
);
audit_case!(
    extra_ring_b,
    CUSTOM,
    CUSTOM.replace("bar D 4 all 1", "bar D 4 all 2"),
    "frozen-ring"
);
audit_case!(
    extra_header_a,
    CUSTOM,
    CUSTOM.replace("x.h", "q.h"),
    "frozen-header"
);
audit_case!(
    extra_header_b,
    CUSTOM,
    CUSTOM.replace("y.h", "q.h"),
    "frozen-header"
);
audit_case!(
    extra_new_a,
    CUSTOM,
    format!("{CUSTOM}a1 F - all 0 x.h\n"),
    "frozen-new"
);
audit_case!(
    extra_new_b,
    CUSTOM,
    format!("{CUSTOM}a2 F - all 0 x.h\n"),
    "frozen-new"
);
audit_case!(
    extra_version_a,
    CUSTOM,
    CUSTOM.replace("# node: RAVEN_1.0", "# node: RAVEN_1.4"),
    "frozen-version"
);
audit_case!(
    extra_version_b,
    CUSTOM,
    CUSTOM.replace("# node: RAVEN_1.0", "# node: RAVEN_1.5"),
    "frozen-version"
);
audit_case!(
    extra_missing_e,
    CUSTOM,
    CUSTOM.replace("foo F - all 0 x.h\n", ""),
    "frozen-missing"
);
audit_case!(
    extra_kind_c,
    CUSTOM,
    CUSTOM.replace("baz F", "baz D"),
    "frozen-kind"
);
