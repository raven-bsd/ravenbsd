// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const FIXTURE: &str = r#"
schema = 1
freeze = "pending"
weights = { optional = 0.5 }
[subset]
smoke5 = ["zlib", "lua", "sqlite", "pcre2", "toybox"]
quick4 = ["zlib", "bzip2", "lua", "dash"]

[[package]]
n = 1
name = "zlib"
tier = 1
build = "cf"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "off_t width, gz* stdio"

[[package]]
n = 2
name = "lua"
tier = 2
build = "mk"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
needs = ["zlib"]
stresses = "strtod, setlocale, popen, tmpnam"

[[package]]
n = 3
name = "sqlite"
tier = 2
build = "ac"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
needs = ["lua"]
stresses = "fcntl locks, mmap, fsync, pread/pwrite, dlopen"

[[package]]
n = 4
name = "pcre2"
tier = 2
build = "cm"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "malloc, JIT mmap"

[[package]]
n = 5
name = "toybox"
tier = 4
build = "kc"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "getmntent, sched_*"

[[package]]
n = 6
name = "bash"
tier = 4
build = "ac"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
stresses = "fork, signals, job control, termios, glob, regex, strftime"

[[package]]
n = 7
name = "bzip2"
tier = 1
build = "mk"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
optional = true
stresses = "plain stdio"

[[package]]
n = 8
name = "dash"
tier = 4
build = "ac"
version = "PLACEHOLDER"
url = "PLACEHOLDER"
sha256 = "PLACEHOLDER"
tarball = "PLACEHOLDER"
optional = true
stresses = "wait, pipe, sigaction, getcwd"
"#;

fn fixture() -> World {
    World::parse(FIXTURE).expect("fixture")
}

fn validated() -> World {
    let world = fixture();
    world.validate_with(8, 7.0).expect("valid");
    world
}

#[test]
fn fixture_world_is_valid() {
    validated();
}

#[test]
fn schema_other_than_one_is_rejected() {
    let mut world = fixture();
    world.schema = 2;
    assert!(world.validate_with(8, 7.0).is_err());
}

#[test]
fn unknown_freeze_state_is_rejected() {
    let mut world = fixture();
    world.freeze = "soon".to_string();
    assert!(world.validate_with(8, 7.0).is_err());
}

#[test]
fn wrong_row_count_is_rejected() {
    let world = fixture();
    let error = world.validate_with(9, 7.5).expect_err("count");
    assert!(error.to_string().contains("9 package rows"));
}

#[test]
fn missing_row_number_is_rejected() {
    let mut world = fixture();
    world.package[7].n = 9;
    let error = world.validate_with(8, 7.0).expect_err("number");
    assert!(error.to_string().contains("outside 1..=8"));
}

#[test]
fn repeated_row_number_is_rejected() {
    let mut world = fixture();
    world.package[7].n = 6;
    let error = world.validate_with(8, 7.0).expect_err("number");
    assert!(error.to_string().contains("repeated"));
}

#[test]
fn repeated_name_is_rejected() {
    let mut world = fixture();
    world.package[7].name = "bash".to_string();
    let error = world.validate_with(8, 7.0).expect_err("name");
    assert!(error.to_string().contains("repeated"));
}

#[test]
fn invalid_name_is_rejected() {
    let mut world = fixture();
    world.package[0].name = "Zlib".to_string();
    let error = world.validate_with(8, 7.0).expect_err("name");
    assert!(error.to_string().contains("not [a-z0-9]"));
}

#[test]
fn unknown_need_is_rejected() {
    let mut world = fixture();
    world.package[0].needs = vec!["nowhere".to_string()];
    let error = world.validate_with(8, 7.0).expect_err("need");
    assert!(error.to_string().contains("unknown package"));
}

#[test]
fn self_need_is_rejected() {
    let mut world = fixture();
    world.package[0].needs = vec!["zlib".to_string()];
    let error = world.validate_with(8, 7.0).expect_err("self");
    assert!(error.to_string().contains("needs itself"));
}

#[test]
fn a_needs_cycle_is_rejected() {
    let mut world = valid_frozen();
    world.package[0].needs = vec!["sqlite".to_string()];
    let error = world.validate_with(8, 7.0).expect_err("cycle");
    assert!(error.to_string().contains("cycle"));
}

#[test]
fn empty_stresses_is_rejected() {
    let mut world = fixture();
    world.package[0].stresses = " ".to_string();
    let error = world.validate_with(8, 7.0).expect_err("stresses");
    assert!(error.to_string().contains("stresses"));
}

#[test]
fn unknown_build_key_is_rejected() {
    let mut world = fixture();
    world.package[0].build = "zz".to_string();
    let error = world.validate_with(8, 7.0).expect_err("build");
    assert!(error.to_string().contains("build key"));
}

#[test]
fn zero_jobs_make_is_rejected() {
    let mut world = fixture();
    world.package[0].jobs_make = 0;
    let error = world.validate_with(8, 7.0).expect_err("jobs");
    assert!(error.to_string().contains("jobs_make"));
}

#[test]
fn unknown_tests_value_is_rejected() {
    let mut world = fixture();
    world.package[0].tests = "run".to_string();
    let error = world.validate_with(8, 7.0).expect_err("tests");
    assert!(error.to_string().contains("tests value"));
}

#[test]
fn a_configure_flag_naming_libc_is_rejected() {
    let mut world = fixture();
    world.package[0].configure = vec!["--disable-glibc-backtrace".to_string()];
    let error = world.validate_with(8, 7.0).expect_err("libc flag");
    assert!(error.to_string().contains("names a libc"));
}

#[test]
fn a_disable_flag_needs_a_note() {
    let mut world = fixture();
    world.package[0].configure = vec!["--disable-nls".to_string()];
    let error = world.validate_with(8, 7.0).expect_err("note");
    assert!(error.to_string().contains("flag_notes"));
}

#[test]
fn a_note_must_start_with_platform_or_libc() {
    let mut world = fixture();
    world.package[0].configure = vec!["--disable-nls".to_string()];
    world.package[0]
        .flag_notes
        .insert("--disable-nls".to_string(), "no catalogs".to_string());
    let error = world.validate_with(8, 7.0).expect_err("note prefix");
    assert!(error.to_string().contains("platform:"));
}

#[test]
fn a_platform_note_is_accepted() {
    let mut world = fixture();
    world.package[0].configure = vec!["--disable-nls".to_string()];
    world.package[0].flag_notes.insert(
        "--disable-nls".to_string(),
        "platform: UTF-8 only, no gettext catalogs".to_string(),
    );
    world.validate_with(8, 7.0).expect("valid");
}

#[test]
fn pending_freeze_rejects_a_real_version() {
    let mut world = fixture();
    world.package[0].version = "1.3.1".to_string();
    let error = world.validate_with(8, 7.0).expect_err("pending");
    assert!(error.to_string().contains("freeze is pending"));
}

#[test]
fn frozen_world_rejects_a_placeholder() {
    let mut world = valid_frozen();
    world.package[0].version = PLACEHOLDER.to_string();
    let error = world.validate_with(8, 7.0).expect_err("frozen");
    assert!(error.to_string().contains("world is frozen"));
}

#[test]
fn frozen_world_rejects_a_bad_sha256() {
    let mut world = valid_frozen();
    world.package[0].sha256 = "ABC".to_string();
    let error = world.validate_with(8, 7.0).expect_err("sha");
    assert!(error.to_string().contains("sha256"));
}

#[test]
fn a_frozen_world_passes_with_real_values() {
    let world = valid_frozen();
    world.validate_with(8, 7.0).expect("valid");
    assert!(world.is_frozen());
}

fn valid_frozen() -> World {
    let mut world = fixture();
    world.freeze = "frozen".to_string();
    for package in &mut world.package {
        package.version = "1.0".to_string();
        package.url = "https://example.invalid/package-1.0.tar.gz".to_string();
        package.sha256 = "0".repeat(64);
        package.tarball = format!("{}.tar.gz", package.name);
    }
    world
}

#[test]
fn wrong_weighted_total_is_rejected() {
    let world = fixture();
    let error = world.validate_with(8, 8.0).expect_err("weight");
    assert!(error.to_string().contains("weighted total"));
}

#[test]
fn optional_rows_count_half() {
    let world = fixture();
    assert!((world.weighted_total() - 7.0).abs() < 1e-9);
    assert_eq!(world.package.iter().filter(|p| p.optional).count(), 2);
}

#[test]
fn unknown_subset_is_rejected() {
    let world = fixture();
    let error = world.subset_names("nope").expect_err("subset");
    assert!(error.to_string().contains("unknown subset"));
}

#[test]
fn a_subset_naming_an_unknown_package_is_rejected() {
    let mut world = fixture();
    world
        .subset
        .insert("bad".to_string(), vec!["nowhere".to_string()]);
    let error = world.validate_with(8, 7.0).expect_err("subset");
    assert!(error.to_string().contains("unknown package"));
}

#[test]
fn named_subsets_resolve() {
    let world = fixture();
    assert_eq!(
        world.subset_names("smoke5").expect("smoke"),
        vec!["zlib", "lua", "sqlite", "pcre2", "toybox"]
    );
    assert_eq!(
        world.subset_names("quick4").expect("quick"),
        vec!["zlib", "bzip2", "lua", "dash"]
    );
}

#[test]
fn tier_subsets_are_computed_from_the_tier_field() {
    let world = fixture();
    assert_eq!(
        world.subset_names("tier1").expect("tier1"),
        vec!["zlib", "bzip2"]
    );
    assert!(world.subset_names("tier6").is_err());
}

#[test]
fn build_order_puts_dependencies_first() {
    let world = fixture();
    let selected = world.select(&[], None, Some("smoke5")).expect("select");
    let names: Vec<&str> = selected
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(names, vec!["zlib", "lua", "sqlite", "pcre2", "toybox"]);
}

#[test]
fn build_order_ties_by_tier_then_row_number() {
    let world = fixture();
    let names = ["dash".to_string(), "bash".to_string(), "toybox".to_string()];
    let selected = world.select(&names, None, None).expect("select");
    let order: Vec<&str> = selected
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    assert_eq!(order, vec!["toybox", "bash", "dash"]);
}

#[test]
fn explicit_selection_deduplicates() {
    let world = fixture();
    let names = ["zlib".to_string(), "zlib".to_string()];
    let selected = world.select(&names, None, None).expect("select");
    assert_eq!(selected.len(), 1);
}

#[test]
fn empty_selection_is_an_error() {
    let world = fixture();
    assert!(world.select(&[], None, None).is_err());
}

#[test]
fn unknown_explicit_package_is_an_error() {
    let world = fixture();
    let names = ["nowhere".to_string()];
    let error = world.select(&names, None, None).expect_err("name");
    assert!(error.to_string().contains("unknown package"));
}

#[test]
fn a_cycle_in_build_order_is_reported() {
    let world = valid_frozen();
    let names = ["zlib".to_string()];
    let mut selected = world.select(&names, None, None).expect("select");
    let second = world.package("lua").expect("lua");
    // Remove the tie by asking for both rows, then point lua back at sqlite via a clone.
    let mut first = selected.remove(0).clone();
    first.needs = vec!["lua".to_string()];
    let mut second = second.clone();
    second.needs = vec!["zlib".to_string()];
    let refs = [&first, &second];
    assert!(world.build_order(&refs).is_err());
}

#[test]
fn expected_tiers_match_the_specification() {
    for n in 22..=47 {
        assert_eq!(expected_tier(n), 1, "row {n}");
    }
    assert_eq!(expected_tier(196), 1);
    for n in 74..=80 {
        assert_eq!(expected_tier(n), 2, "row {n}");
    }
    for n in 83..=100 {
        assert_eq!(expected_tier(n), 2, "row {n}");
    }
    for n in 101..=133 {
        assert_eq!(expected_tier(n), 3, "row {n}");
    }
    for n in [81, 82, 145, 146, 147, 180, 181, 182, 183, 184, 185] {
        assert_eq!(expected_tier(n), 5, "row {n}");
    }
    for n in [1, 21, 48, 73, 134, 179, 186, 200] {
        assert_eq!(expected_tier(n), 4, "row {n}");
    }
    assert_eq!(expected_tier(195), 4);
}

#[test]
fn the_shipped_world_has_two_hundred_rows() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("world.toml");
    let world = World::load(&path).expect("world.toml");
    world.validate().expect("valid world.toml");
    assert_eq!(world.package.len(), 200);
    assert!((world.weighted_total() - 197.5).abs() < 1e-9);
    assert_eq!(world.freeze, "pending");
    assert_eq!(
        world.subset_names("smoke5").expect("smoke5"),
        vec!["zlib", "lua", "sqlite", "pcre2", "toybox"]
    );
    assert_eq!(
        world.subset_names("quick4").expect("quick4"),
        vec!["zlib", "bzip2", "lua", "dash"]
    );
    assert_eq!(
        world
            .package
            .iter()
            .filter(|package| package.optional)
            .map(|package| package.n)
            .collect::<Vec<_>>(),
        vec![182, 183, 184, 188, 195]
    );
    assert_eq!(world.package("zlib").expect("zlib").n, 25);
    assert_eq!(world.package("lua").expect("lua").n, 76);
    assert_eq!(world.package("sqlite").expect("sqlite").n, 99);
    assert_eq!(world.package("pcre2").expect("pcre2").n, 100);
    assert_eq!(world.package("toybox").expect("toybox").n, 7);
}
