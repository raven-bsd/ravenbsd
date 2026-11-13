// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the five symbol-list grammars and the manual miner.

use super::*;
use crate::testutil::fixture;

#[test]
fn openbsd_strips_multiline_and_trailing_comments() {
    let text = "/* header\n   over two lines */\nfoo\nbar /* trailing */\nbaz/*x*/\n\n";
    let parsed = parse_openbsd(text);
    assert_eq!(
        parsed.names.iter().cloned().collect::<Vec<_>>(),
        vec!["bar", "baz", "foo"]
    );
    assert_eq!(parsed.identifier_lines, 3);
}

#[test]
fn openbsd_fixture_has_one_name_per_line() {
    let parsed = parse_file(&fixture("lists/Symbols.list"), parse_openbsd).unwrap();
    assert!(parsed.names.contains("strlcpy"));
    assert!(parsed.names.contains("_yp_check"));
    assert_eq!(parsed.names.len(), parsed.identifier_lines);
}

#[test]
fn freebsd_records_version_and_private_flag() {
    let text = "\
FBSD_1.0 {
    global:
        foo;
        bar;
    local:
        *;
};

FBSDprivate_1.0 {
    hidden_one;
} FBSD_1.0;
";
    let parsed = parse_freebsd(text);
    assert_eq!(
        parsed.names.iter().cloned().collect::<Vec<_>>(),
        vec!["bar", "foo", "hidden_one"]
    );
    assert_eq!(
        parsed.versions.get("foo").map(String::as_str),
        Some("FBSD_1.0")
    );
    assert!(parsed.private.contains("hidden_one"));
    assert!(!parsed.private.contains("foo"));
}

#[test]
fn freebsd_ignores_local_global_and_star_lines() {
    let text = "FBSD_1.1 { global: local: *; };\n";
    let parsed = parse_freebsd(text);
    assert!(parsed.names.is_empty());
    assert_eq!(parsed.identifier_lines, 0);
}

#[test]
fn freebsd_accepts_a_single_line_block() {
    let text = "FBSD_1.2 { global: one; two; };\n";
    let parsed = parse_freebsd(text);
    assert!(parsed.names.contains("one"));
    assert!(parsed.names.contains("two"));
}

#[test]
fn bionic_keeps_global_names_and_arch_notes() {
    let text = "\
LIBC {
  global:
    foo; # arm x86 introduced=23
    bar;
  local:
    hidden; # arm
    *;
};
";
    let parsed = parse_bionic(text);
    assert!(parsed.names.contains("foo"));
    assert!(parsed.names.contains("bar"));
    assert!(!parsed.names.contains("hidden"));
    assert_eq!(
        parsed.notes.get("foo").map(String::as_str),
        Some("arm x86 introduced=23")
    );
    assert_eq!(parsed.identifier_lines, 2);
}

#[test]
fn bionic_star_line_is_not_a_name() {
    let text = "LIBC { global: *; };\n";
    let parsed = parse_bionic(text);
    assert!(parsed.names.is_empty());
}

#[test]
fn illumos_keywords_are_never_names() {
    let text = "\
$mapfile_version 2
# a comment
SYMBOL_VERSION ILLUMOS_0.1 {
    global:
        foo;
        bar = FLAGS(0x1);
        baz { ASSERT(1, 2); };
        ALIAS(alias_name);
        SIZE(0x10);
    local:
        *;
} ILLUMOS_0.0;
";
    let parsed = parse_illumos(text);
    assert_eq!(
        parsed.names.iter().cloned().collect::<Vec<_>>(),
        vec!["bar", "baz", "foo"]
    );
    assert!(!parsed.names.contains("alias_name"));
    assert!(!parsed.names.contains("FLAGS"));
}

#[test]
fn illumos_skips_a_multi_line_assertion_block() {
    let text = "\
SYMBOL_VERSION V1 {
    global:
        keeper {
            ASSERT(1, 1, 1, 1, 1);
            TYPE(1);
        };
        after;
    local:
        *;
};
";
    let parsed = parse_illumos(text);
    assert!(parsed.names.contains("keeper"));
    assert!(parsed.names.contains("after"));
    assert!(!parsed.names.contains("ASSERT"));
    assert!(!parsed.names.contains("TYPE"));
}

#[test]
fn glibc_manual_reads_functions_and_headers() {
    let text = "\
@deftypefun size_t strlen (const char *@var{s})
@standards{ISO, string.h}
@safety{@prelim{}}

@deftypefunx {void *} memcpy (void *, const void *, size_t)
@standards{POSIX.1, string.h}

@deftypefn {Macro} {char *} strdupa (const char *)

@deftypevr Macro int ARGP_ERR_UNKNOWN
@standards{GNU, argp.h}

@deftypevr {Variable} {volatile int} errno
@standards{POSIX.1, errno.h}
";
    let output = parse_manual(text);
    assert_eq!(output.deftype_lines, 5);
    assert_eq!(output.entries["strlen"].kind, Kind::Func);
    assert!(output.entries["strlen"].standard.contains("iso-c"));
    assert_eq!(output.entries["strlen"].header.as_deref(), Some("string.h"));
    assert!(output.entries["memcpy"].standard.contains("posix"));
    assert_eq!(output.entries["strdupa"].kind, Kind::Func);
    assert_eq!(output.entries["ARGP_ERR_UNKNOWN"].kind, Kind::Macro);
    assert_eq!(output.entries["errno"].kind, Kind::Data);
}

#[test]
fn glibc_manual_standardsx_wins_over_the_definition_line() {
    let text = "\
@deftypevr Macro int SECOND_TAG
@standardsx{SECOND_TAG, C99, stdint.h}
";
    let output = parse_manual(text);
    let entry = &output.entries["SECOND_TAG"];
    assert_eq!(entry.kind, Kind::Macro);
    assert!(entry.standard.contains("iso-c"));
    assert_eq!(entry.header.as_deref(), Some("stdint.h"));
    assert_eq!(output.one_field_standards, 0);
}

#[test]
fn glibc_manual_skips_one_field_standards() {
    let text = "\
@deftypefun int onefield (void)
@standards{GNU}
@deftypefun int twofield (void)
@standards{C99, string.h}
";
    let output = parse_manual(text);
    assert_eq!(output.one_field_standards, 1);
    assert!(output.entries.contains_key("onefield"));
    assert!(output.entries["onefield"].standard.is_empty());
    assert!(output.entries["twofield"].standard.contains("iso-c"));
}

#[test]
fn glibc_manual_texi_fixture_parses() {
    let text = fs::read_to_string(fixture("manual/string.texi")).unwrap();
    let output = parse_manual(&text);
    assert!(output.deftype_lines >= 3);
    assert!(output.entries.contains_key("strlcpy"));
}

#[test]
fn parse_file_reports_a_missing_path() {
    let result = parse_file(Path::new("definitely/not/here"), parse_openbsd);
    assert!(result.is_err());
}
