// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the `size` lint keys and `profiles_pinned`.

use super::*;

struct Fixture {
    dir: crate::libc_lint_testutil::Tree,
}

impl Fixture {
    fn new(_label: &str) -> Self {
        let dir = crate::libc_lint_testutil::Tree::new();
        dir.write(
            "tests/libc/size/hooks.toml",
            "[[hook]]\nhook = \"__rl_fmt_float\"\nstrong_member = \"fmt_float.o\"\n",
        );
        Self { dir }
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        self.dir.write(relative, text);
        self.dir.root().join(relative)
    }

    fn check(&self, relative: &str) -> Vec<Violation> {
        let path = self.dir.root().join(relative);
        let mut violations = Vec::new();
        run(self.dir.root(), &[path], &mut violations).expect("run");
        violations
    }
}

#[test]
fn one_symbol_accepts_the_stem() {
    let fixture = Fixture::new("lint-one-ok");
    fixture.write(
        "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs",
        "#[unsafe(no_mangle)]\npub extern \"C\" fn strlen() -> usize { 0 }\n",
    );
    assert!(
        fixture
            .check("libc/ravenlibc/crates/raven-libc/src/string/strlen.rs")
            .is_empty()
    );
}

#[test]
fn one_symbol_rejects_a_different_first_symbol() {
    let fixture = Fixture::new("lint-one-stem");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    fixture.write(
        path,
        "#[unsafe(no_mangle)]\npub extern \"C\" fn strnlen() -> usize { 0 }\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "one-symbol" && violation.message.contains("strnlen"))
    );
}

#[test]
fn one_symbol_rejects_five_aliases() {
    let fixture = Fixture::new("lint-one-five");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    let mut text = String::new();
    for name in [
        "strlen",
        "strlen_l",
        "strlen_unlocked",
        "_strlen",
        "strlen_extra",
    ] {
        text.push_str(&format!(
            "#[unsafe(no_mangle)]\npub extern \"C\" fn {name}() -> usize {{ 0 }}\n"
        ));
    }
    fixture.write(path, &text);
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "one-symbol"
                && violation.message.contains("5 exported"))
    );
}

#[test]
fn one_symbol_accepts_a_leading_underscore_file() {
    let fixture = Fixture::new("lint-one-underscore");
    let path = "libc/ravenlibc/crates/raven-libc/src/unistd/_exit.rs";
    fixture.write(
        path,
        "#[unsafe(no_mangle)]\npub extern \"C\" fn _exit(status: i32) -> ! { loop {} }\n",
    );
    assert!(fixture.check(path).is_empty());
}

#[test]
fn one_symbol_honours_the_allowlist() {
    let fixture = Fixture::new("lint-one-allow");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strtok.rs";
    fixture.write(
        path,
        "#[unsafe(no_mangle)]\npub extern \"C\" fn strtok_r() -> usize { 0 }\n",
    );
    fixture.dir.write(
        "tests/libc/size/size-allowlist.toml",
        &format!("[[one-symbol]]\nfile = \"{path}\"\nreason = \"test\"\n"),
    );
    assert!(fixture.check(path).is_empty());
}

#[test]
fn no_mangle_scope_flags_a_logic_crate() {
    let fixture = Fixture::new("lint-scope");
    let path = "libc/ravenlibc/crates/raven-libc-str/src/lib.rs";
    fixture.write(
        path,
        "#[unsafe(no_mangle)]\npub extern \"C\" fn strlen() -> usize { 0 }\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "no-mangle-scope")
    );
}

#[test]
fn no_mangle_scope_accepts_the_shell() {
    let fixture = Fixture::new("lint-scope-shell");
    fixture.write(
        "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs",
        "#[unsafe(no_mangle)]\npub extern \"C\" fn strlen() -> usize { 0 }\n",
    );
    let violations = fixture.check("libc/ravenlibc/crates/raven-libc/src/string/strlen.rs");
    assert!(
        !violations
            .iter()
            .any(|violation| violation.key == "no-mangle-scope")
    );
}

#[test]
fn sections_flags_link_section_on_a_function() {
    let fixture = Fixture::new("lint-sections");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    fixture.write(
        path,
        "#[unsafe(link_section = \".text.foo\")]\npub extern \"C\" fn strlen() -> usize { 0 }\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "sections")
    );
}

#[test]
fn sections_flags_gc_disabling_text() {
    let fixture = Fixture::new("lint-sections-gc");
    let path = "tools/libc-example/Cargo.toml";
    fixture.write(path, "[build]\nrustflags = [\"-C\", \"link-dead-code\"]\n");
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "sections")
    );
}

#[test]
fn no_fmt_flags_each_token() {
    for token in [
        "write!(f, \"x\")",
        "format!(\"x\")",
        "to_string()",
        "assert_eq!(a, b)",
    ] {
        let fixture = Fixture::new("lint-fmt");
        let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
        fixture.write(
            path,
            &format!("#[unsafe(no_mangle)]\npub fn strlen() {{ let _ = {token}; }}\n"),
        );
        let violations = fixture.check(path);
        assert!(
            violations.iter().any(|violation| violation.key == "no-fmt"),
            "{token}"
        );
    }
}

#[test]
fn no_fmt_flags_derive_debug() {
    let fixture = Fixture::new("lint-fmt-derive");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    fixture.write(path, "#[derive(Clone, Debug)]\npub struct S;\n");
    let violations = fixture.check(path);
    assert!(violations.iter().any(|violation| violation.key == "no-fmt"));
}

#[test]
fn no_fmt_skips_ut_files() {
    let fixture = Fixture::new("lint-fmt-ut");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen_ut.rs";
    fixture.write(path, "fn t() { assert_eq!(1, 1); }\n");
    assert!(fixture.check(path).is_empty());
}

#[test]
fn no_fmt_does_not_fire_on_a_comment_or_string() {
    let fixture = Fixture::new("lint-fmt-comment");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    fixture.write(
        path,
        "// write!(f, \"x\") is forbidden\nconst S: &str = \"format!(x)\";\n",
    );
    assert!(fixture.check(path).is_empty());
}

#[test]
fn panic_handler_flags_two_handlers() {
    let fixture = Fixture::new("lint-panic-two");
    let path = "libc/ravenlibc/crates/raven-libc/src/panic.rs";
    fixture.write(
        path,
        "#[panic_handler]\nfn a(_: &core::panic::PanicInfo) -> ! { loop {} }\n#[panic_handler]\nfn b(_: &core::panic::PanicInfo) -> ! { loop {} }\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "panic-handler"
                && violation.message.contains("2 handlers"))
    );
}

#[test]
fn panic_handler_flags_a_body_naming_info() {
    let fixture = Fixture::new("lint-panic-info");
    let path = "libc/ravenlibc/crates/raven-libc/src/panic.rs";
    fixture.write(
        path,
        "#[panic_handler]\nfn a(info: &core::panic::PanicInfo) -> ! { let _ = info; loop {} }\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "panic-handler" && violation.message.contains("info"))
    );
}

#[test]
fn panic_handler_accepts_the_real_shape() {
    let fixture = Fixture::new("lint-panic-ok");
    let path = "libc/ravenlibc/crates/raven-libc/src/panic.rs";
    fixture.write(
        path,
        "#[panic_handler]\nfn panic(_payload: &Payload<'_>) -> ! { write_fixed(); loop {} }\n",
    );
    assert!(fixture.check(path).is_empty());
}

#[test]
fn no_registry_flags_inventory_and_function_pointer_arrays() {
    let fixture = Fixture::new("lint-registry");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    fixture.write(
        path,
        "inventory::submit! { Item }\nstatic HOOKS: [fn(); 4] = [a, b, c, d];\n",
    );
    let violations = fixture.check(path);
    assert_eq!(
        violations
            .iter()
            .filter(|violation| violation.key == "no-registry")
            .count(),
        2
    );
}

#[test]
fn statics_flags_a_lazy_cell() {
    let fixture = Fixture::new("lint-statics");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strlen.rs";
    fixture.write(path, "static S: OnceLock<u8> = OnceLock::new();\n");
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "statics")
    );
}

#[test]
fn hooks_flags_an_unknown_name() {
    let fixture = Fixture::new("lint-hooks-unknown");
    let path = "libc/ravenlibc/crates/raven-libc/src/stdio/vprintf.rs";
    fixture.write(
        path,
        "weak_hook! {\n    pub(crate) fn __rl_unknown() -> ();\n}\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "hooks" && violation.message.contains("__rl_unknown"))
    );
}

#[test]
fn hooks_flags_a_missing_strong_member() {
    let fixture = Fixture::new("lint-hooks-missing");
    let path = "libc/ravenlibc/crates/raven-libc/src/stdio/vprintf.rs";
    fixture.write(
        path,
        "weak_hook! {\n    pub(crate) fn __rl_fmt_float() -> ();\n}\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations.iter().any(
            |violation| violation.key == "hooks" && violation.message.contains("strong member")
        )
    );
}

#[test]
fn hooks_accepts_a_known_hook_with_its_member() {
    let fixture = Fixture::new("lint-hooks-ok");
    let path = "libc/ravenlibc/crates/raven-libc/src/stdio/vprintf.rs";
    fixture.write(
        path,
        "weak_hook! {\n    pub(crate) fn __rl_fmt_float() -> ();\n}\n",
    );
    fixture.write(
        "libc/ravenlibc/crates/raven-libc/src/hooks/fmt_float.rs",
        "pub extern \"C\" fn __rl_fmt_float_impl() {}\n",
    );
    let violations = fixture.check(path);
    assert!(!violations.iter().any(|violation| violation.key == "hooks"));
}

#[test]
fn reloc_table_flags_a_string_pointer_table() {
    let fixture = Fixture::new("lint-reloc");
    let path = "libc/ravenlibc/crates/raven-libc/src/string/strerror.rs";
    fixture.write(
        path,
        "static NAMES: &'static [&'static str] = &[\"a\", \"b\"];\n",
    );
    let violations = fixture.check(path);
    assert!(
        violations
            .iter()
            .any(|violation| violation.key == "reloc-table")
    );
}

#[test]
fn dyn_flags_the_shell_and_honours_the_allowlist() {
    let fixture = Fixture::new("lint-dyn");
    let path = "libc/ravenlibc/crates/raven-libc/src/glob/glob.rs";
    fixture.write(path, "fn f(x: &mut dyn FnMut()) {}\n");
    assert!(
        fixture
            .check(path)
            .iter()
            .any(|violation| violation.key == "dyn")
    );
    fixture.dir.write(
        "tests/libc/size/size-allowlist.toml",
        &format!("[[dyn]]\nfile = \"{path}\"\nreason = \"test\"\n"),
    );
    assert!(
        !fixture
            .check(path)
            .iter()
            .any(|violation| violation.key == "dyn")
    );
}

#[test]
fn profiles_pinned_passes_on_the_real_root_manifest() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = std::fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml");
    assert_eq!(profiles_pinned(&text), Vec::<String>::new());
}

#[test]
fn profiles_pinned_fails_on_a_mutated_copy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = std::fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml");
    let mutated = text.replace(
        "[profile.libc-a]\ninherits = \"release\"\nopt-level = \"z\"",
        "[profile.libc-a]\ninherits = \"release\"\nopt-level = \"3\"",
    );
    assert!(!profiles_pinned(&mutated).is_empty());
}

#[test]
fn r8_leaf_crates_are_core_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = std::fs::read_to_string(root.join("libc/ravenlibc/xtask/crates.toml"))
        .expect("crates.toml");
    let value: toml::Value = toml::from_str(&text).expect("parse");
    let rows = value
        .get("crate")
        .and_then(toml::Value::as_array)
        .expect("crate table");
    for name in [
        "raven-libc-str",
        "raven-libc-ctype",
        "raven-libc-m",
        "raven-libc-dtoa",
        "raven-libc-fmt",
        "raven-libc-ssp",
        "raven-libc-sys",
    ] {
        let row = rows
            .iter()
            .find(|row| row.get("name").and_then(toml::Value::as_str) == Some(name))
            .unwrap_or_else(|| panic!("{name} missing"));
        assert_eq!(
            row.get("std").and_then(toml::Value::as_str),
            Some("core"),
            "{name} must be core-only (R-8); crates.toml wins over doc 16 for elf"
        );
    }
    let elf = rows
        .iter()
        .find(|row| row.get("name").and_then(toml::Value::as_str) == Some("raven-libc-elf"))
        .expect("elf");
    assert_eq!(elf.get("std").and_then(toml::Value::as_str), Some("alloc"));
}
