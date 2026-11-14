// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for rules K1-K7, with a fake `nm`/`readelf` and one real rustc archive.

use super::*;
use crate::testutil;

const MAP: &str = "\
RAVEN_1.0 {
\tglobal:
\t\ta;
\t\tb;
};
RAVENprivate_1.0 {
\tglobal:
\t\t__rl_x;
};
";

const LIST: &str = "\
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# public
a
b
# private
__rl_x
";

const NM_OK: &str = "\
libc.a[raven_libc.aaa-cgu.0.rcgu.o]:
a T 0 4
__rl_x D 0 8

libc.a[raven_libc.bbb-cgu.0.rcgu.o]:
b T 0 4
";

const HOOKS_EMPTY: &str = "# no hook\n";

const EXPECTED_OK: &str = "\
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
a
b
__rl_x
";

fn setup(
    tag: &str,
    nm_output: &str,
    hooks: &str,
    expected: &str,
    deferred: &str,
) -> (testutil::TempDir, CheckOptions) {
    let dir = testutil::tempdir(tag);
    let versions = testutil::write(
        &dir.path,
        "Versions.def",
        "RAVEN_1.0 { };\nRAVENprivate_1.0 { } RAVEN_1.0;\n",
    );
    let map = testutil::write(&dir.path, "Symbol.map", MAP);
    let list = testutil::write(&dir.path, "Symbols.list", LIST);
    let hooks_path = testutil::write(&dir.path, "hooks.toml", hooks);
    testutil::write(&dir.path, "expected/string.txt", expected);
    testutil::write(&dir.path, "expected/DEFERRED.txt", deferred);
    let nm = testutil::script(
        &dir.path,
        "fake-nm",
        &format!("#!/bin/sh\ncat <<'EOF'\n{nm_output}EOF\n"),
    );
    let readelf = testutil::script(&dir.path, "fake-readelf", "#!/bin/sh\nexit 0\n");
    let options = CheckOptions {
        root: dir.path.clone(),
        archive: dir.path.join("libc.a"),
        list,
        maps: vec![map],
        versions,
        hooks: hooks_path,
        expected: dir.path.join("expected"),
        arch: "x86_64".to_string(),
        so: None,
        nm: nm.display().to_string(),
        readelf: readelf.display().to_string(),
    };
    (dir, options)
}

fn rules(findings: &[Finding]) -> Vec<&str> {
    findings.iter().map(|finding| finding.rule).collect()
}

#[test]
fn k1_to_k6_pass_on_a_well_formed_fixture() {
    let (_dir, options) = setup("check-pass", NM_OK, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, summary) = run(&options).expect("check");
    assert!(findings.is_empty(), "{findings:?}");
    assert_eq!(summary.members, 2);
    assert_eq!(summary.shell_members, 2);
    assert_eq!(summary.public, 2);
    assert_eq!(summary.private, 1);
}

#[test]
fn k1_sees_an_extra_listed_name() {
    let dir = testutil::tempdir("k1-extra");
    let (_d, mut options) = setup(
        "k1-extra-inner",
        NM_OK,
        HOOKS_EMPTY,
        EXPECTED_OK,
        "# none\n",
    );
    options.list = testutil::write(&dir.path, "Symbols.list", &format!("{LIST}extra\n"));
    let (findings, _) = run(&options).expect("check");
    assert!(rules(&findings).contains(&"K1"));
}

#[test]
fn k1_sees_a_duplicate_public_name() {
    let dir = testutil::tempdir("k1-dup");
    let (_d, mut options) = setup("k1-dup-inner", NM_OK, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    options.list = testutil::write(
        &dir.path,
        "Symbols.list",
        "# SPDX-License-Identifier: BSD-3-Clause\na\nb\na\n# private\n__rl_x\n",
    );
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("duplicate"))
    );
}

#[test]
fn k2_rejects_a_weak_public_definition() {
    let nm = NM_OK.replace("a T 0 4", "a W 0 4");
    let (_dir, options) = setup("k2-weak", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K2" && finding.message.contains("weak"))
    );
}

#[test]
fn k2_rejects_a_second_public_definition() {
    let nm = format!("{NM_OK}\nlibc.a[raven_libc.ccc-cgu.0.rcgu.o]:\na T 0 4\n");
    let (_dir, options) = setup("k2-second", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K2" && finding.message.contains("strong"))
    );
}

#[test]
fn k2_rejects_a_missing_public_definition() {
    let nm = NM_OK.replace("b T 0 4", "");
    let (_dir, options) = setup("k2-missing", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("not defined"))
    );
}

#[test]
fn k3_rejects_an_exported_helper() {
    let nm = format!("{NM_OK}helper T 0 4\n");
    let (_dir, options) = setup("k3-helper", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K3" && finding.message.contains("helper"))
    );
}

#[test]
fn k3_rejects_a_public_name_from_another_crate() {
    let nm = format!("{NM_OK}\nlibc.a[core-1.core.2-cgu.0.rcgu.o]:\na T 0 4\n");
    let (_dir, options) = setup("k3-foreign", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K3" && finding.message.contains("non-shell"))
    );
}

#[test]
fn k3_tolerates_rust_mangled_and_private_names() {
    let nm =
        format!("{NM_OK}_ZN3foo3barE T 0 4\n_RNvNtCs1234_7___rustc8foo T 0 4\n__rl_hook T 0 4\n");
    let (_dir, options) = setup("k3-mangled", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(!rules(&findings).contains(&"K3"), "{findings:?}");
}

#[test]
fn k3_tolerates_the_hidden_signal_entry() {
    let nm = format!("{NM_OK}__raven_sigentry T 0 4\n");
    let (_dir, options) = setup("k3-sigentry", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(!rules(&findings).contains(&"K3"), "{findings:?}");
}

#[test]
fn k4_rejects_five_text_symbols_in_one_member() {
    let nm = format!("{NM_OK}a1 T 0 4\na2 T 0 4\na3 T 0 4\na4 T 0 4\n");
    let (_dir, options) = setup("k4-five", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K4" && finding.message.contains("5 global text"))
    );
}

#[test]
fn k4_rejects_two_data_symbols_in_one_member() {
    let nm = NM_OK.replace("__rl_x D 0 8", "__rl_x D 0 8\n__rl_y D 0 8");
    let (_dir, options) = setup("k4-data", &nm, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K4" && finding.message.contains("2 global data"))
    );
}

#[test]
fn k5_accepts_a_strong_hook_in_its_own_member() {
    let hooks = "[[hook]]\nname = \"__rl_h\"\nstrong_symbol = \"__rl_h_impl\"\nstrong_file = \"src/hooks/h.rs\"\npulled_by = [\"a\"]\n";
    let nm = format!("{NM_OK}\nlibc.a[raven_libc.ddd-cgu.0.rcgu.o]:\n__rl_h_impl T 0 4\n");
    let (_dir, options) = setup("k5-pass", &nm, hooks, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn k5_rejects_a_hook_defined_in_the_callers_member() {
    let hooks = "[[hook]]\nname = \"__rl_h\"\nstrong_symbol = \"__rl_h_impl\"\nstrong_file = \"src/hooks/h.rs\"\npulled_by = [\"a\"]\n";
    let nm = "libc.a[raven_libc.aaa-cgu.0.rcgu.o]:\na T 0 4\n__rl_h_impl T 0 4\n\nlibc.a[raven_libc.bbb-cgu.0.rcgu.o]:\nb T 0 4\n__rl_x D 0 8\n";
    let (_dir, options) = setup("k5-fail", nm, hooks, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("share member"))
    );
}

#[test]
fn k5_rejects_a_hook_without_a_strong_definition() {
    let hooks = "[[hook]]\nname = \"__rl_h\"\nstrong_symbol = \"__rl_h_impl\"\nstrong_file = \"src/hooks/h.rs\"\n";
    let (_dir, options) = setup("k5-missing", NM_OK, hooks, EXPECTED_OK, "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("no strong definition"))
    );
}

#[test]
fn k6_accepts_a_deferred_name_with_a_reason() {
    let (_dir, options) = setup(
        "k6-deferred",
        NM_OK,
        HOOKS_EMPTY,
        EXPECTED_OK,
        "zzz no logic function\n",
    );
    let (findings, _) = run(&options).expect("check");
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn k6_rejects_an_expected_name_that_is_neither_public_nor_deferred() {
    let (_dir, options) = setup("k6-unknown", NM_OK, HOOKS_EMPTY, "nope\n", "# none\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == "K6" && finding.message.contains("nope"))
    );
}

#[test]
fn k6_rejects_a_deferred_name_without_a_reason() {
    let (_dir, options) = setup("k6-reason", NM_OK, HOOKS_EMPTY, EXPECTED_OK, "zzz\n");
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("without a reason"))
    );
}

fn readelf_script(dir: &std::path::Path, dyn_syms: &str, relocs: &str) -> String {
    testutil::write(dir, "dyn.txt", dyn_syms);
    testutil::write(dir, "rel.txt", relocs);
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\n  --dyn-syms) cat \"{}/dyn.txt\" ;;\n  *) cat \"{}/rel.txt\" ;;\nesac\n",
        dir.display(),
        dir.display()
    );
    testutil::script(dir, "readelf", &script)
        .display()
        .to_string()
}

const DYN_OK: &str = "\
Num: Value Size Type Bind Vis Ndx Name
1: 0000000000001000 4 FUNC GLOBAL DEFAULT 1 a@@RAVEN_1.0
2: 0000000000001008 4 FUNC GLOBAL DEFAULT 1 b@@RAVEN_1.0
3: 0000000000001010 8 OBJECT GLOBAL DEFAULT 1 __rl_x@@RAVENprivate_1.0
";

#[test]
fn k7_passes_on_versioned_dynamic_symbols() {
    let dir = testutil::tempdir("k7-pass");
    let (_d, mut options) = setup("k7-pass-inner", NM_OK, HOOKS_EMPTY, EXPECTED_OK, "# none\n");
    options.readelf = readelf_script(&dir.path, DYN_OK, "no relocations\n");
    options.so = Some(dir.path.join("libc.so.1"));
    let (findings, _) = run(&options).expect("check");
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn k7_rejects_an_unversioned_symbol() {
    let dir = testutil::tempdir("k7-unversioned");
    let (_d, mut options) = setup(
        "k7-unversioned-inner",
        NM_OK,
        HOOKS_EMPTY,
        EXPECTED_OK,
        "# none\n",
    );
    let dyn_syms = DYN_OK.replace("a@@RAVEN_1.0", "a");
    options.readelf = readelf_script(&dir.path, &dyn_syms, "");
    options.so = Some(dir.path.join("libc.so.1"));
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("unversioned"))
    );
}

#[test]
fn k7_rejects_too_many_jump_slots() {
    let dir = testutil::tempdir("k7-jumps");
    let (_d, mut options) = setup(
        "k7-jumps-inner",
        NM_OK,
        HOOKS_EMPTY,
        EXPECTED_OK,
        "# none\n",
    );
    let relocs = (0..41)
        .map(|index| format!("{index:016x} R_X86_64_JUMP_SLOT 0 sym{index} + 0\n"))
        .collect::<String>();
    options.readelf = readelf_script(&dir.path, DYN_OK, &relocs);
    options.so = Some(dir.path.join("libc.so.1"));
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("JUMP_SLOT"))
    );
}

#[test]
fn k7_rejects_a_missing_public_name() {
    let dir = testutil::tempdir("k7-missing");
    let (_d, mut options) = setup(
        "k7-missing-inner",
        NM_OK,
        HOOKS_EMPTY,
        EXPECTED_OK,
        "# none\n",
    );
    let dyn_syms = DYN_OK.replace(
        "2: 0000000000001008 4 FUNC GLOBAL DEFAULT 1 b@@RAVEN_1.0\n",
        "",
    );
    options.readelf = readelf_script(&dir.path, &dyn_syms, "");
    options.so = Some(dir.path.join("libc.so.1"));
    let (findings, _) = run(&options).expect("check");
    assert!(
        findings
            .iter()
            .any(|finding| finding.message.contains("missing from"))
    );
}

#[test]
fn a_real_staticlib_fixture_passes_k1_to_k6() {
    if !testutil::rustc_available() || !testutil::nm_available() {
        testutil::skip("rustc or nm is missing");
        return;
    }
    let dir = testutil::tempdir("real-archive");
    let source = "\
#![no_std]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
#[unsafe(no_mangle)]
pub extern \"C\" fn a() -> i32 { 1 }
#[unsafe(no_mangle)]
pub static mut __rl_x: u32 = 0;
";
    let Some(archive) = testutil::build_staticlib(&dir.path, "raven_libc", source) else {
        return;
    };
    let map = "\
RAVEN_1.0 {
\tglobal:
\t\ta;
};
RAVENprivate_1.0 {
\tglobal:
\t\t__rl_x;
};
";
    let list = "# SPDX-License-Identifier: BSD-3-Clause\na\n# private\n__rl_x\n";
    let versions = testutil::write(
        &dir.path,
        "Versions.def",
        "RAVEN_1.0 { };\nRAVENprivate_1.0 { } RAVEN_1.0;\n",
    );
    let map_path = testutil::write(&dir.path, "Symbol.map", map);
    let list_path = testutil::write(&dir.path, "Symbols.list", list);
    let hooks = testutil::write(&dir.path, "hooks.toml", "# none\n");
    testutil::write(&dir.path, "expected/string.txt", "a\n__rl_x\n");
    testutil::write(&dir.path, "expected/DEFERRED.txt", "# none\n");
    let options = CheckOptions {
        root: dir.path.clone(),
        archive,
        list: list_path,
        maps: vec![map_path],
        versions,
        hooks,
        expected: dir.path.join("expected"),
        arch: "x86_64".to_string(),
        so: None,
        nm: "nm".to_string(),
        readelf: "readelf".to_string(),
    };
    let (findings, _) = run(&options).expect("check");
    assert!(findings.is_empty(), "{findings:?}");
}
