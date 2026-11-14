// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::Framework;

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "raven-atf-gen-make-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(&path, text).expect("write");
    }
    root
}

fn programs(scan: &MakeScan) -> Vec<&str> {
    scan.programs
        .iter()
        .map(|program| program.name.as_str())
        .collect()
}

#[test]
fn reads_tests_c() {
    let root = tree(
        "tests-c",
        &[
            (
                "Makefile",
                ".include <bsd.test.mk>\nTESTS_C+= t_one t_two\n",
            ),
            ("t_one.c", "int main(void){return 0;}\n"),
            ("t_two.c", "int main(void){return 0;}\n"),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(programs(&scan), ["t_one", "t_two"]);
    assert_eq!(scan.programs[0].framework, Framework::Atf);
}

#[test]
fn reads_atf_tests_c() {
    let root = tree(
        "atf",
        &[
            ("Makefile", "ATF_TESTS_C+= foo_test\n"),
            ("foo_test.c", "int main(void){return 0;}\n"),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(programs(&scan), ["foo_test"]);
    assert_eq!(scan.programs[0].sources, ["foo_test.c"]);
}

#[test]
fn reads_netbsd_atf_tests_c() {
    let root = tree(
        "netbsd-atf",
        &[
            ("Makefile", "NETBSD_ATF_TESTS_C+= bar_test\n"),
            ("bar_test.c", "int main(void){return 0;}\n"),
        ],
    );
    assert_eq!(programs(&super::scan(&root).expect("scan")), ["bar_test"]);
}

#[test]
fn reads_plain_tests_c_as_exit_status() {
    let root = tree(
        "plain",
        &[
            ("Makefile", "PLAIN_TESTS_C+= plain\n"),
            ("plain.c", "int main(void){return 0;}\n"),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(scan.programs[0].framework, Framework::ExitStatus);
}

#[test]
fn reads_prog_and_default_source() {
    let root = tree(
        "prog",
        &[
            ("Makefile", "PROG= hello\n"),
            ("hello.c", "int main(void){return 0;}\n"),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(programs(&scan), ["hello"]);
    assert_eq!(scan.programs[0].framework, Framework::ExitStatus);
}

#[test]
fn reads_prog_srcs() {
    let root = tree(
        "srcs",
        &[
            ("Makefile", "PROG= hello\nSRCS= hello.c extra.c\n"),
            ("hello.c", ""),
            ("extra.c", ""),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(scan.programs[0].sources, ["hello.c", "extra.c"]);
}

#[test]
fn reads_srcs_for_one_program() {
    let root = tree(
        "srcs-name",
        &[
            ("Makefile", "ATF_TESTS_C+= one\nSRCS.one= a.c b.c\n"),
            ("one.c", ""),
            ("a.c", ""),
            ("b.c", ""),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(scan.programs[0].sources, ["a.c", "b.c"]);
}

#[test]
fn cflags_keep_only_the_whitelist() {
    let root = tree(
        "cflags",
        &[
            (
                "Makefile",
                "ATF_TESTS_C+= one\nCFLAGS+= -DFOO -Iinc -std=gnu11 -Wall -O2 -fno-builtin -pthread\n",
            ),
            ("one.c", ""),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(
        scan.programs[0].cflags,
        ["-DFOO", "-Iinc", "-std=gnu11", "-Wall"]
    );
}

#[test]
fn copts_for_one_program_are_kept() {
    let root = tree(
        "copts",
        &[
            (
                "Makefile",
                "ATF_TESTS_C+= one\nCOPTS.one.c+= -Wno-stringop-overread\n",
            ),
            ("one.c", ""),
        ],
    );
    assert_eq!(
        super::scan(&root).expect("scan").programs[0].cflags,
        ["-Wno-stringop-overread"]
    );
}

#[test]
fn ldadd_keep_only_the_whitelist() {
    let root = tree(
        "ldadd",
        &[
            (
                "Makefile",
                "ATF_TESTS_C+= one\nLDADD+= -lm -lfoo -lpthread -Wl,-z,now -lutil\n",
            ),
            ("one.c", ""),
        ],
    );
    assert_eq!(
        super::scan(&root).expect("scan").programs[0].ldadd,
        ["-lm", "-lpthread", "-lutil"]
    );
}

#[test]
fn subdir_recurses() {
    let root = tree(
        "subdir",
        &[
            ("Makefile", "SUBDIR+= sub\n"),
            ("sub/Makefile", "PROG= child\n"),
            ("sub/child.c", ""),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(programs(&scan), ["sub/child"]);
    assert_eq!(scan.programs[0].sources, ["sub/child.c"]);
}

#[test]
fn tests_subdirs_recurses() {
    let root = tree(
        "test-subdirs",
        &[
            ("Makefile", "TESTS_SUBDIRS+= alpha\n"),
            ("alpha/Makefile", "TESTS_C+= t_alpha\n"),
            ("alpha/t_alpha.c", ""),
        ],
    );
    assert_eq!(
        programs(&super::scan(&root).expect("scan")),
        ["alpha/t_alpha"]
    );
}

#[test]
fn wait_is_not_a_subdirectory() {
    let root = tree(
        "wait",
        &[
            ("Makefile", "SUBDIR+= .WAIT one\n"),
            ("one/Makefile", "PROG= p\n"),
            ("one/p.c", ""),
        ],
    );
    assert_eq!(programs(&super::scan(&root).expect("scan")), ["one/p"]);
}

#[test]
fn tests_sh_are_recorded() {
    let root = tree(
        "shell",
        &[
            ("Makefile", "TESTS_SH+= t_shell\n"),
            ("t_shell.sh", "#!/bin/sh\n"),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert!(scan.programs.is_empty());
    assert_eq!(scan.shell_tests, ["t_shell.sh"]);
}

#[test]
fn a_program_under_a_conditional_is_unparsed() {
    let root = tree(
        "conditional",
        &[
            (
                "Makefile",
                ".if ${HAVE_X} == \"yes\"\nATF_TESTS_C+= maybe\n.endif\n",
            ),
            ("maybe.c", ""),
        ],
    );
    let scan = super::scan(&root).expect("scan");
    assert_eq!(scan.programs[0].status, "unparsed");
    assert!(
        scan.programs[0]
            .status_reason
            .as_deref()
            .unwrap_or("")
            .contains(".if")
    );
}

#[test]
fn a_missing_source_is_unparsed() {
    let root = tree("missing", &[("Makefile", "ATF_TESTS_C+= gone\n")]);
    let scan = super::scan(&root).expect("scan");
    assert_eq!(scan.programs[0].status, "unparsed");
    assert!(
        scan.programs[0]
            .status_reason
            .as_deref()
            .unwrap_or("")
            .contains("source not vendored")
    );
}

#[test]
fn a_non_bsd_include_is_ignored_for_program_discovery() {
    let root = tree(
        "include",
        &[
            ("Makefile", ".include \"local.mk\"\nATF_TESTS_C+= one\n"),
            ("local.mk", ""),
            ("one.c", ""),
        ],
    );
    assert_eq!(programs(&super::scan(&root).expect("scan")), ["one"]);
}

#[test]
fn continuations_are_joined() {
    let root = tree(
        "continuation",
        &[
            ("Makefile", "ATF_TESTS_C+= one\nCFLAGS+= -DA \\\n\t-DB\n"),
            ("one.c", ""),
        ],
    );
    assert_eq!(
        super::scan(&root).expect("scan").programs[0].cflags,
        ["-DA", "-DB"]
    );
}

#[test]
fn tabs_are_accepted() {
    let root = tree(
        "tab",
        &[("Makefile", "ATF_TESTS_C+=\tone\n"), ("one.c", "")],
    );
    assert_eq!(programs(&super::scan(&root).expect("scan")), ["one"]);
}

#[test]
fn make_variable_flags_are_dropped() {
    let root = tree(
        "variable",
        &[
            (
                "Makefile",
                "ATF_TESTS_C+= one\nCFLAGS+= ${CC_WNO_STRINGOP} -DREAL\n",
            ),
            ("one.c", ""),
        ],
    );
    assert_eq!(
        super::scan(&root).expect("scan").programs[0].cflags,
        ["-DREAL"]
    );
}

#[test]
fn a_directory_without_a_makefile_is_skipped() {
    let root = tree("nomake", &[("empty/keep.txt", "")]);
    assert!(super::scan(&root).expect("scan").programs.is_empty());
}

#[test]
fn the_makefile_path_is_not_read_as_a_program() {
    let root = tree(
        "sources",
        &[
            ("Makefile", "ATF_TESTS_C+= one\nSRCS.one = one.c\n"),
            ("one.c", ""),
        ],
    );
    assert_eq!(
        super::scan(&root).expect("scan").programs[0].sources,
        ["one.c"]
    );
}

#[test]
fn strip_c_is_idempotent() {
    assert_eq!(strip_c("t_x.c"), "t_x");
    assert_eq!(strip_c("t_x"), "t_x");
}

#[test]
fn join_continuations_keeps_a_space() {
    let joined = join_continuations("A=1 \\\n  2\n");
    assert!(joined.starts_with("A=1") && joined.contains('2'));
}

#[test]
fn joining_an_unfinished_continuation_is_safe() {
    let joined = join_continuations("A=1 \\");
    assert_eq!(joined, "A=1 \n");
}

#[test]
fn a_missing_directory_is_empty() {
    let root = Path::new("/nonexistent/raven-atf-run-gen-make");
    assert!(super::scan(root).expect("scan").programs.is_empty());
}
