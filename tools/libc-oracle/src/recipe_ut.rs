// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::collections::BTreeSet;

use super::*;
use crate::testutil::TempDir;

fn valid_recipe() -> String {
    r#"schema = 1
family = "basic"
description = "two tiny functions"
exports = ["one", "two"]
opt = ["O2", "O0"]
defines = ["EXTRA(x)=(x)"]
include = ["donor:openbsd/lib/libc/string", "shim:openbsd"]
flags = ["-fno-common", "-std=gnu11"]

[[source]]
donor = "openbsd"
path = "lib/libc/string/one.c"

[[source]]
donor = "musl"
path = "src/string/two.c"
"#
    .to_string()
}

fn donors() -> BTreeSet<String> {
    ["openbsd", "musl"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn load(text: &str) -> Result<Recipe> {
    let temp = TempDir::new("recipe");
    temp.write("recipes/basic/oracle.toml", text);
    load_family(&temp.path().join("recipes"), "basic", Some(&donors()))
}

#[test]
fn valid_recipe_loads() {
    let recipe = load(&valid_recipe()).expect("recipe");
    assert_eq!(recipe.family, "basic");
    assert_eq!(recipe.exports, vec!["one", "two"]);
    assert_eq!(recipe.opt, vec!["O2", "O0"]);
    assert_eq!(recipe.source.len(), 2);
}

#[test]
fn unknown_key_is_an_error() {
    let mut text = valid_recipe();
    text.push_str("\nmystery = 1\n");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn unsorted_exports_are_an_error() {
    let text = valid_recipe().replace("[\"one\", \"two\"]", "[\"two\", \"one\"]");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("sorted and unique"));
}

#[test]
fn duplicate_exports_are_an_error() {
    let text = valid_recipe().replace("[\"one\", \"two\"]", "[\"one\", \"one\"]");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("sorted and unique"));
}

#[test]
fn parent_component_in_a_source_path_is_an_error() {
    let text = valid_recipe().replace(
        "path = \"lib/libc/string/one.c\"",
        "path = \"../lib/libc/string/one.c\"",
    );
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("must be relative"));
}

#[test]
fn assembler_source_is_refused() {
    let text = valid_recipe().replace("one.c", "one.S");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("end in .c"));
}

#[test]
fn optimisation_flag_is_refused() {
    let text = valid_recipe().replace(
        "flags = [\"-fno-common\"",
        "flags = [\"-O3\", \"-fno-common\"",
    );
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("not whitelisted"));
}

#[test]
fn include_flag_is_refused() {
    let text = valid_recipe().replace(
        "flags = [\"-fno-common\"",
        "flags = [\"-Ifoo\", \"-fno-common\"",
    );
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("not whitelisted"));
}

#[test]
fn unknown_donor_is_an_error() {
    let text = valid_recipe().replace("donor = \"musl\"", "donor = \"plan9\"");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("not in donors.lock"));
}

#[test]
fn family_must_match_the_directory() {
    let text = valid_recipe().replace("family = \"basic\"", "family = \"other\"");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("does not match"));
}

#[test]
fn family_name_is_restricted() {
    let text = valid_recipe().replace("family = \"basic\"", "family = \"Basic\"");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("[a-z0-9-]+"));
}

#[test]
fn duplicate_object_stems_are_an_error() {
    let text = valid_recipe().replace("src/string/two.c", "src/string/one.c");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("object stem"));
}

#[test]
fn schema_must_be_one() {
    let text = valid_recipe().replace("schema = 1", "schema = 2");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("schema 2"));
}

#[test]
fn empty_exports_are_an_error() {
    let text = valid_recipe().replace("exports = [\"one\", \"two\"]", "exports = []");
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("exports is empty"));
}

#[test]
fn missing_source_is_an_error() {
    let start = valid_recipe().find("[[source]]").unwrap();
    let text = valid_recipe()[..start].to_string();
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("at least one [[source]]"));
}

#[test]
fn bad_include_is_an_error() {
    let text = valid_recipe().replace(
        "include = [\"donor:openbsd/lib/libc/string\", \"shim:openbsd\"]",
        "include = [\"absolute:/usr/include\"]",
    );
    let error = load(&text).unwrap_err();
    assert!(format!("{error:#}").contains("invalid include"));
}

#[test]
fn defaults_are_read_next_to_the_recipes_directory() {
    let temp = TempDir::new("defaults");
    temp.write(
        "defaults.toml",
        "schema = 1\ndefines = [\"DEF_WEAK(x)=\", \"__RCSID(x)=\"]\n",
    );
    let defaults = read_defaults(&temp.path().join("recipes")).expect("defaults");
    assert_eq!(defaults.defines.len(), 2);
}

#[test]
fn defaults_schema_is_checked() {
    let temp = TempDir::new("defaults-schema");
    temp.write("defaults.toml", "schema = 2\ndefines = []\n");
    let error = read_defaults(&temp.path().join("recipes")).unwrap_err();
    assert!(format!("{error:#}").contains("schema 2"));
}

#[test]
fn opt_list_accepts_known_levels_and_rejects_others() {
    assert_eq!(parse_opt_list("O2,O0").unwrap(), vec!["O2", "O0"]);
    assert!(parse_opt_list("O4").is_err());
    assert!(parse_opt_list("").is_err());
    assert!(parse_opt_list("O2,O2").is_err());
}

#[test]
fn flag_whitelist_accepts_the_documented_forms() {
    for flag in [
        "-fno-builtin",
        "-fPIC",
        "-fwrapv",
        "-fvisibility=hidden",
        "-std=gnu11",
        "-Wall",
        "-Werror=implicit-function-declaration",
        "-w",
        "-m64",
    ] {
        assert!(allowed_flag(flag), "{flag}");
    }
    for flag in ["-O3", "-Ifoo", "-include", "fPIC", "-DNAME=1", "-W"] {
        assert_eq!(allowed_flag(flag), flag == "-W", "{flag}");
    }
}

#[test]
fn load_all_returns_sorted_families() {
    let temp = TempDir::new("all");
    temp.write(
        "recipes/zeta/oracle.toml",
        &valid_recipe().replace("family = \"basic\"", "family = \"zeta\""),
    );
    temp.write(
        "recipes/alpha/oracle.toml",
        &valid_recipe().replace("family = \"basic\"", "family = \"alpha\""),
    );
    let families = load_all(&temp.path().join("recipes"), Some(&donors())).expect("recipes");
    let names: Vec<&str> = families
        .iter()
        .map(|recipe| recipe.family.as_str())
        .collect();
    assert_eq!(names, vec!["alpha", "zeta"]);
}

#[test]
fn model_only_recipe_has_no_sources() {
    let text = r#"schema = 1
family = "basic"
description = "model differential"
model = true
exports = []
"#;
    let recipe = load(text).expect("model recipe");
    assert!(recipe.model);
    assert!(recipe.source.is_empty());
}

#[test]
fn header_recipe_accepts_only_headers() {
    let text = r#"schema = 1
family = "basic"
description = "header only"
type = "header"
exports = []

[[source]]
donor = "openbsd"
path = "lib/libc/include/tiny.h"
"#;
    let recipe = load(text).expect("header recipe");
    assert_eq!(recipe.kind, "header");
    assert_eq!(recipe.source.len(), 1);
}
