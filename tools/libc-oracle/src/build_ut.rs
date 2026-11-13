// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::failure::exit_code;
use crate::recipe::{Defaults, Source, read_defaults};
use crate::testutil::TempDir;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn recipe(name: &str) -> Recipe {
    crate::recipe::load_family(&fixtures().join("recipes"), name, None).expect("recipe")
}

fn defaults() -> Defaults {
    read_defaults(&fixtures().join("recipes")).expect("defaults")
}

fn cc_available() -> bool {
    Command::new("cc")
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
}

fn require_cc() -> bool {
    if cc_available() {
        return true;
    }
    if std::env::var_os("RL_ORACLE_REQUIRED").is_some_and(|value| value == "1") {
        panic!("RL_ORACLE_REQUIRED=1 but cc not found");
    }
    println!("SKIP: cc not found");
    false
}

fn nm(path: &Path, args: &[&str]) -> String {
    let output = Command::new("nm")
        .arg(path)
        .args(args)
        .output()
        .expect("nm");
    assert!(
        output.status.success(),
        "nm: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn nm_output_is_parsed_and_undefined_symbols_are_ignored() {
    let text = "0000000000000000 T alpha\n                 U memcpy\n0000000000000010 D beta\n";
    assert_eq!(parse_nm(text).unwrap(), vec!["alpha", "beta"]);
    assert!(parse_nm("garbage\n").is_err());
}

#[test]
fn compile_args_follow_the_documented_order() {
    let recipe = recipe("fixture");
    let defaults = defaults();
    let request = BuildRequest {
        recipe: &recipe,
        defaults: &defaults,
        recipes: &fixtures().join("recipes"),
        resources: &fixtures(),
        out: Path::new("build"),
        opt: vec!["O2".to_string()],
        sanitize: false,
        no_pin_check: true,
        adhoc: false,
    };
    let (actual, recorded) = compile_args(
        "O2",
        &request,
        &recipe.source[0],
        &[("shim:openbsd".to_string(), "/abs/shim".to_string())],
        Path::new("/abs/donor/lib/helper.c"),
        "obj/helper.o",
    );
    assert_eq!(
        actual,
        vec![
            "-fno-builtin",
            "-fPIC",
            "-w",
            "-std=gnu11",
            "-O2",
            "-DDEF_WEAK(x)=",
            "-I/abs/shim",
            "-c",
            "/abs/donor/lib/helper.c",
            "-o",
            "obj/helper.o",
        ]
    );
    assert_eq!(
        recorded,
        vec![
            "-fno-builtin",
            "-fPIC",
            "-w",
            "-std=gnu11",
            "-O2",
            "-DDEF_WEAK(x)=",
            "-Ishim:openbsd",
            "-c",
            "fixture:lib/helper.c",
            "-o",
            "obj/helper.o",
        ]
    );
}

#[test]
fn fixture_build_matches_nm_exports_and_keeps_memcpy_external() {
    if !require_cc() {
        return;
    }
    let temp = TempDir::new("build-ok");
    let recipe = recipe("fixture");
    let defaults = defaults();
    let recipes = fixtures().join("recipes");
    let out = temp.path().join("build");
    let request = BuildRequest {
        recipe: &recipe,
        defaults: &defaults,
        recipes: &recipes,
        resources: &fixtures(),
        out: &out,
        opt: vec!["O2".to_string()],
        sanitize: false,
        no_pin_check: true,
        adhoc: false,
    };
    let directories = build(&request).expect("build");
    assert_eq!(directories.len(), 1);
    let directory = &directories[0];
    let library = directory.join("liboracle_fixture.so");
    assert!(library.is_file());
    let exports = fs::read_to_string(directory.join("exports.txt")).expect("exports");
    assert_eq!(exports, "oracle_fixture_api\noracle_fixture_helper\n");
    let all = nm(&library, &["-D"]);
    assert!(all.contains("oracle_fixture_api"));
    assert!(all.contains("oracle_fixture_helper"));
    assert!(all.contains(" memcpy"), "{all}");
    assert!(!all.contains("oracle_memcpy"), "{all}");
    assert!(!all.contains(" fixture_helper"), "{all}");
    let json = fs::read_to_string(directory.join("build.json")).expect("build.json");
    assert!(json.contains("\"family\":\"fixture\""));
    assert!(json.contains("\"opt\":\"O2\""));
    assert!(!json.contains("openbsd"));
    let temp_text = temp.path().display().to_string();
    assert!(!json.contains(&temp_text), "{json}");
}

#[test]
fn a_recipe_that_forgets_a_helper_is_refused_naming_it() {
    if !require_cc() {
        return;
    }
    let temp = TempDir::new("build-extra");
    let recipe = recipe("fixture-extra");
    let defaults = defaults();
    let recipes = fixtures().join("recipes");
    let request = BuildRequest {
        recipe: &recipe,
        defaults: &defaults,
        recipes: &recipes,
        resources: &fixtures(),
        out: &temp.path().join("build"),
        opt: vec!["O2".to_string()],
        sanitize: false,
        no_pin_check: true,
        adhoc: false,
    };
    let error = build(&request).unwrap_err();
    assert_eq!(exit_code(&error), 1);
    let text = format!("{error}");
    assert!(text.contains("exports mismatch"));
    assert!(text.contains("fixture_helper"));
}

#[test]
fn o0_and_o2_agree_and_repeated_builds_are_byte_identical() {
    if !require_cc() {
        return;
    }
    let temp = TempDir::new("build-repro");
    let recipe = recipe("fixture");
    let defaults = defaults();
    let recipes = fixtures().join("recipes");
    let out = temp.path().join("build");
    let request = BuildRequest {
        recipe: &recipe,
        defaults: &defaults,
        recipes: &recipes,
        resources: &fixtures(),
        out: &out,
        opt: vec!["O2".to_string(), "O0".to_string()],
        sanitize: false,
        no_pin_check: true,
        adhoc: false,
    };
    let directories = build(&request).expect("build");
    let o2 = directories[0].join("exports.txt");
    let o0 = directories[1].join("exports.txt");
    assert_eq!(fs::read(&o2).unwrap(), fs::read(&o0).unwrap());
    let first_json = fs::read(directories[0].join("build.json")).unwrap();
    let first_exports = fs::read(&o2).unwrap();
    build(&request).expect("second build");
    assert_eq!(fs::read(&o2).unwrap(), first_exports);
    assert_eq!(
        fs::read(directories[0].join("build.json")).unwrap(),
        first_json
    );
}

#[test]
fn adhoc_build_derives_exports_from_nm() {
    if !require_cc() {
        return;
    }
    let temp = TempDir::new("build-adhoc");
    let recipe = Recipe {
        schema: 1,
        family: "adhoc-single".to_string(),
        description: "ad hoc".to_string(),
        exports: Vec::new(),
        opt: vec!["O2".to_string()],
        defines: Vec::new(),
        include: Vec::new(),
        flags: Vec::new(),
        source: vec![Source {
            donor: "fixture".to_string(),
            path: "lib/single.c".to_string(),
        }],
    };
    let defaults = defaults();
    let recipes = fixtures().join("recipes");
    let request = BuildRequest {
        recipe: &recipe,
        defaults: &defaults,
        recipes: &recipes,
        resources: &fixtures(),
        out: &temp.path().join("build"),
        opt: vec!["O2".to_string()],
        sanitize: false,
        no_pin_check: true,
        adhoc: true,
    };
    let directories = build(&request).expect("adhoc build");
    let exports = fs::read_to_string(directories[0].join("exports.txt")).expect("exports");
    assert_eq!(exports, "oracle_fixture_single\n");
}
