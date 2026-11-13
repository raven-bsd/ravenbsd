// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::failure::exit_code;

fn always(_program: &str) -> bool {
    true
}

fn never(_program: &str) -> bool {
    false
}

#[test]
fn defaults_are_the_plain_tool_names() {
    let tools = discover_with(&|_| None, &always).expect("tools");
    assert_eq!(
        tools,
        Tools {
            cc: "cc".to_string(),
            ld: "ld".to_string(),
            nm: "nm".to_string(),
            objcopy: "objcopy".to_string(),
        }
    );
}

#[test]
fn environment_overrides_are_honoured() {
    let tools = discover_with(
        &|name| match name {
            "CC" => Some("/opt/cc".to_string()),
            "LD" => Some("/opt/ld".to_string()),
            "NM" => Some("/opt/nm".to_string()),
            "OBJCOPY" => Some("/opt/objcopy".to_string()),
            _ => None,
        },
        &always,
    )
    .expect("tools");
    assert_eq!(tools.cc, "/opt/cc");
    assert_eq!(tools.ld, "/opt/ld");
    assert_eq!(tools.nm, "/opt/nm");
    assert_eq!(tools.objcopy, "/opt/objcopy");
}

#[test]
fn objcopy_falls_back_to_llvm_objcopy() {
    let tools = discover_with(&|_| None, &|program| program != "objcopy").expect("tools");
    assert_eq!(tools.objcopy, "llvm-objcopy");
}

#[test]
fn a_missing_cc_is_a_skip_with_exit_77() {
    let error = discover_with(&|_| None, &never).unwrap_err();
    assert_eq!(exit_code(&error), 77);
    assert_eq!(error.to_string(), "skipped: cc not found");
}

#[test]
fn a_failing_explicit_objcopy_is_a_skip() {
    let error = discover_with(
        &|name| (name == "OBJCOPY").then(|| "/missing/objcopy".to_string()),
        &|program| program != "/missing/objcopy",
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "skipped: objcopy not found");
}
