// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the patch classifier.

use std::collections::BTreeMap;
use std::path::Path;

use super::*;

fn rings() -> BTreeMap<String, u8> {
    BTreeMap::from([
        ("strlcpy".to_string(), 1),
        ("epoll_create".to_string(), 2),
        ("memchr".to_string(), 0),
    ])
}

#[test]
fn a_libc_header_include_is_classified() {
    let patch = "\
--- a/src/x.c
+++ b/src/x.c
@@ -1,2 +1,3 @@
 #include <stdio.h>
+#include <sys/epoll.h>
";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(result.categories.contains(&Category::LibcHeader));
    assert!(result.headers.contains("sys/epoll.h"));
}

#[test]
fn a_plain_c_header_is_not_libc_header() {
    let patch = "+#include <stdio.h>\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(!result.categories.contains(&Category::LibcHeader));
}

#[test]
fn libc_identity_macros_are_classified() {
    let patch = "+#ifdef __GLIBC__\n-#if defined(_GNU_SOURCE)\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(result.categories.contains(&Category::LibcMacro));
    assert!(result.macros.contains("__GLIBC__"));
    assert!(result.macros.contains("_GNU_SOURCE"));
}

#[test]
fn non_standard_symbol_calls_are_classified() {
    let patch = "+    strlcpy (dst, src, size);\n+    n = epoll_create(1);\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(result.categories.contains(&Category::LibcSymbol));
    assert!(result.symbols.contains("strlcpy"));
    assert!(result.symbols.contains("epoll_create"));
}

#[test]
fn standard_symbols_are_excluded_from_libc_symbol() {
    let patch = "+    memchr (s, c, n);\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(!result.categories.contains(&Category::LibcSymbol));
    assert!(result.symbols.is_empty());
}

#[test]
fn a_symbol_without_a_call_is_not_a_symbol_hit() {
    let patch = "+    const char *name = \"strlcpy\";\n+    int strlcpy_value = 1;\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(!result.categories.contains(&Category::LibcSymbol));
}

#[test]
fn semantics_is_the_fallback() {
    let patch = "\
--- glibc/README
+++ glibc/README
@@ -1 +1 @@
-old text
+new text
";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert_eq!(result.categories.len(), 1);
    assert!(result.categories.contains(&Category::LibcSemantics));
}

#[test]
fn the_file_name_can_select_semantics() {
    let patch = "+ one changed line\n";
    let result = classify_patch(Path::new("fix-libc-portab.patch"), patch, &rings());
    assert!(result.categories.contains(&Category::LibcSemantics));
}

#[test]
fn a_patch_can_fire_two_categories() {
    let patch = "+#include <sys/queue.h>\n+    strlcpy (a, b, c);\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(result.categories.contains(&Category::LibcHeader));
    assert!(result.categories.contains(&Category::LibcSymbol));
    assert_eq!(result.categories.len(), 2);
}

#[test]
fn removed_lines_count_and_header_lines_do_not() {
    let patch = "--- a/x\n+++ b/x\n-#include <linux/limits.h>\n";
    let result = classify_patch(Path::new("fix.patch"), patch, &rings());
    assert!(result.categories.contains(&Category::LibcHeader));
    assert!(!result.categories.contains(&Category::LibcSemantics));
}
