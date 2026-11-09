// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `PROVENANCE.toml` schema, import coverage, notices, and the donors lock.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path};

use serde::Deserialize;

use crate::model::Violation;
use crate::spdx;

/// One `[[import]]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Import {
    pub path: String,
    pub upstream: String,
    pub upstream_path: String,
    pub upstream_rev: String,
    pub license: String,
    pub tier: String,
    pub copied: String,
    // Part of the schema contract; no current rule consumes these two values.
    #[allow(dead_code)]
    pub modified: bool,
    #[allow(dead_code)]
    #[serde(default)]
    pub notes: Option<String>,
}

/// A loaded `PROVENANCE.toml`.
pub(crate) struct Loaded {
    pub imports: Vec<Import>,
    pub violations: Vec<Violation>,
    /// False when the file exists but could not be parsed. `fix-spdx` refuses to run then.
    pub usable: bool,
}

/// Load and validate `<root>/PROVENANCE.toml`.
pub(crate) fn load(root: &Path) -> Loaded {
    let path = root.join("PROVENANCE.toml");
    let mut loaded = Loaded {
        imports: Vec::new(),
        violations: Vec::new(),
        usable: true,
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            loaded.violations.push(Violation::new(
                "PROVENANCE.toml",
                "provenance-schema",
                "file is missing",
            ));
            return loaded;
        }
        Err(error) => {
            loaded.usable = false;
            loaded.violations.push(Violation::new(
                "PROVENANCE.toml",
                "provenance-schema",
                format!("cannot read file: {error}"),
            ));
            return loaded;
        }
    };
    let value: toml::Value = match toml::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            loaded.usable = false;
            loaded.violations.push(Violation::new(
                "PROVENANCE.toml",
                "provenance-schema",
                format!("invalid TOML: {error}"),
            ));
            return loaded;
        }
    };
    let Some(table) = value.as_table() else {
        loaded.usable = false;
        loaded.violations.push(Violation::new(
            "PROVENANCE.toml",
            "provenance-schema",
            "top level must be a table",
        ));
        return loaded;
    };
    for key in table.keys() {
        if key != "import" {
            loaded.violations.push(Violation::new(
                "PROVENANCE.toml",
                "provenance-schema",
                format!("unknown top-level field `{key}`"),
            ));
        }
    }
    let Some(import_value) = table.get("import") else {
        return loaded;
    };
    let Some(entries) = import_value.as_array() else {
        loaded.usable = false;
        loaded.violations.push(Violation::new(
            "PROVENANCE.toml",
            "provenance-schema",
            "`import` must be an array of tables",
        ));
        return loaded;
    };

    for entry in entries {
        let guessed_path = entry
            .get("path")
            .and_then(toml::Value::as_str)
            .unwrap_or("PROVENANCE.toml");
        match entry.clone().try_into::<Import>() {
            Ok(import) => {
                loaded.violations.extend(validate(root, &import));
                loaded.imports.push(import);
            }
            Err(error) => loaded.violations.push(Violation::new(
                guessed_path,
                "provenance-schema",
                error.to_string(),
            )),
        }
    }

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for import in &loaded.imports {
        *counts.entry(import.path.as_str()).or_default() += 1;
    }
    for (path, count) in counts {
        if count > 1 {
            loaded.violations.push(Violation::new(
                path,
                "provenance-duplicate",
                format!("path is listed {count} times"),
            ));
        }
    }
    loaded
}

fn validate(root: &Path, import: &Import) -> Vec<Violation> {
    let path = import.path.clone();
    let schema = |message: String| Violation::new(path.clone(), "provenance-schema", message);
    let mut violations = Vec::new();

    if let Some(problem) = path_problem(&import.path) {
        violations.push(schema(problem));
    } else if !root.join(&import.path).exists() {
        violations.push(Violation::new(
            path.clone(),
            "provenance-path",
            "path does not exist in the repository",
        ));
    }
    for (field, value) in [
        ("upstream", &import.upstream),
        ("upstream_path", &import.upstream_path),
    ] {
        if value.trim().is_empty() {
            violations.push(schema(format!("`{field}` must not be empty")));
        }
    }
    if !is_sha(&import.upstream_rev) {
        violations.push(schema(
            "`upstream_rev` must be 40 hexadecimal characters".to_string(),
        ));
    }
    if !valid_date(&import.copied) {
        violations.push(schema(format!(
            "`copied` is not a valid YYYY-MM-DD date: {}",
            import.copied
        )));
    }
    if spdx::parse_expression(&import.license).is_err() {
        violations.push(schema(format!(
            "`license` is not a valid SPDX expression: {}",
            import.license
        )));
    }
    match import.tier.as_str() {
        "A" => {}
        "B" => {
            if !tier_b_allowed(&import.path) {
                violations.push(Violation::new(
                    path.clone(),
                    "provenance-tier-b",
                    "tier B imports are only allowed under addons/ or crates/third-party-*/",
                ));
            }
        }
        "C" => violations.push(Violation::new(
            path.clone(),
            "provenance-tier-c",
            "tier C imports are forbidden",
        )),
        other => violations.push(schema(format!(
            "`tier` must be \"A\", \"B\" or \"C\", found `{other}`"
        ))),
    }
    violations
}

fn path_problem(path: &str) -> Option<String> {
    if path.trim().is_empty() {
        return Some("`path` must not be empty".to_string());
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Some("`path` must be repository-relative".to_string());
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Some("`path` must not contain `..` components".to_string());
    }
    None
}

fn tier_b_allowed(path: &str) -> bool {
    if path.trim_end_matches('/').starts_with("addons/") {
        return true;
    }
    let trimmed = path.trim_end_matches('/');
    let Some(rest) = trimmed.strip_prefix("crates/third-party-") else {
        return false;
    };
    !rest.split('/').next().unwrap_or("").is_empty()
}

fn is_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_date(value: &str) -> bool {
    let mut parts = value.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return false;
    }
    if !year.bytes().all(|byte| byte.is_ascii_digit())
        || !month.bytes().all(|byte| byte.is_ascii_digit())
        || !day.bytes().all(|byte| byte.is_ascii_digit())
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        year.parse::<u32>(),
        month.parse::<u32>(),
        day.parse::<u32>(),
    ) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    day <= days
}

/// Find the import entry covering `file`, preferring the longest path prefix.
pub(crate) fn find_entry<'a>(imports: &'a [Import], file: &str) -> Option<&'a Import> {
    imports
        .iter()
        .filter(|entry| {
            let prefix = entry.path.trim_end_matches('/');
            !prefix.is_empty()
                && (file == prefix
                    || file
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.starts_with('/')))
        })
        .max_by_key(|entry| entry.path.trim_end_matches('/').len())
}

/// Whether the first 10 lines carry a copyright or matching-licence notice.
pub(crate) fn has_notice(content: &str, license: &str) -> bool {
    content.lines().take(10).any(|line| {
        line.contains("Copyright")
            || spdx::find_spdx(line, 1)
                .is_some_and(|expression| spdx::expressions_compatible(&expression, license))
    })
}

/// One donors lock record.
#[derive(Debug, Clone)]
pub(crate) struct LockEntry {
    pub sha: String,
}

/// Read a donors lock, or `Ok(None)` when the file does not exist.
pub(crate) fn load_lock(path: &Path) -> Result<Option<HashMap<String, LockEntry>>, std::io::Error> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(parse_lock(&text))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn parse_lock(text: &str) -> HashMap<String, LockEntry> {
    let mut entries = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(name), Some(sha)) = (fields.next(), fields.next()) else {
            continue;
        };
        entries.insert(
            name.to_string(),
            LockEntry {
                sha: sha.to_string(),
            },
        );
    }
    entries
}

/// Check every import against the donors lock.
pub(crate) fn check_lock(imports: &[Import], lock: &HashMap<String, LockEntry>) -> Vec<Violation> {
    let mut violations = Vec::new();
    for import in imports {
        match lock.get(&import.upstream) {
            None => violations.push(Violation::new(
                &import.path,
                "provenance-lock",
                format!(
                    "upstream `{}` is not listed in the donors lock",
                    import.upstream
                ),
            )),
            Some(entry) if entry.sha != import.upstream_rev => violations.push(Violation::new(
                &import.path,
                "provenance-lock",
                format!(
                    "upstream_rev {} does not match donors lock revision {}",
                    import.upstream_rev, entry.sha
                ),
            )),
            Some(_) => {}
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn entry(path: &str, tier: &str, license: &str) -> String {
        format!(
            "[[import]]\npath = \"{path}\"\nupstream = \"redox-kernel\"\n\
             upstream_path = \"src/lib.rs\"\nupstream_rev = \"{SHA}\"\n\
             license = \"{license}\"\ntier = \"{tier}\"\ncopied = \"2026-11-09\"\n\
             modified = true\n"
        )
    }

    fn root_with(files: &[&str]) -> TempDir {
        let temp = TempDir::new("prov");
        for file in files {
            temp.write(file, "// Copyright (c) 2026, Someone\n");
        }
        temp
    }

    fn load_text(temp: &TempDir, text: &str) -> Loaded {
        temp.write("PROVENANCE.toml", text);
        load(temp.path())
    }

    #[test]
    fn valid_entry_has_no_violations() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let loaded = load_text(&temp, &entry("crates/foo/src/lib.rs", "A", "MIT"));
        assert!(loaded.violations.is_empty(), "{:?}", loaded.violations);
        assert_eq!(loaded.imports.len(), 1);
    }

    #[test]
    fn missing_required_field_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text =
            entry("crates/foo/src/lib.rs", "A", "MIT").replace("upstream = \"redox-kernel\"\n", "");
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("upstream"))
        );
    }

    #[test]
    fn unknown_field_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = format!(
            "{}extra = \"nope\"\n",
            entry("crates/foo/src/lib.rs", "A", "MIT")
        );
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("unknown field"))
        );
    }

    #[test]
    fn unknown_top_level_field_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = format!(
            "[[imports]]\npath = \"x\"\n{}",
            entry("crates/foo/src/lib.rs", "A", "MIT")
        );
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("unknown top-level"))
        );
    }

    #[test]
    fn bad_date_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = entry("crates/foo/src/lib.rs", "A", "MIT").replace("2026-11-09", "2026-02-30");
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("copied"))
        );
    }

    #[test]
    fn leap_day_is_accepted() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = entry("crates/foo/src/lib.rs", "A", "MIT").replace("2026-11-09", "2028-02-29");
        let loaded = load_text(&temp, &text);
        assert!(
            !loaded
                .violations
                .iter()
                .any(|v| v.message.contains("copied"))
        );
    }

    #[test]
    fn date_validation_table() {
        for valid in ["2026-11-09", "2024-02-29", "2000-02-29", "1999-12-31"] {
            assert!(valid_date(valid), "{valid} should be valid");
        }
        for invalid in [
            "2026-13-01",
            "2026-00-01",
            "2025-02-29",
            "1900-02-29",
            "2026-04-31",
            "2026-1-09",
            "2026-11-9",
            "2026/11/09",
            "0000-01-01",
        ] {
            assert!(!valid_date(invalid), "{invalid} should be invalid");
        }
    }

    #[test]
    fn bad_sha_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = entry("crates/foo/src/lib.rs", "A", "MIT")
            .replace(SHA, "0123456789abcdef0123456789abcdef0123456");
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("upstream_rev"))
        );
    }

    #[test]
    fn non_hex_sha_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = entry("crates/foo/src/lib.rs", "A", "MIT").replace(SHA, &"z".repeat(40));
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("upstream_rev"))
        );
    }

    #[test]
    fn tier_c_is_rejected() {
        let temp = root_with(&["addons/foo/file.c"]);
        let loaded = load_text(&temp, &entry("addons/foo/file.c", "C", "GPL-2.0"));
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-tier-c")
        );
    }

    #[test]
    fn tier_b_outside_allowed_roots_is_rejected() {
        let temp = root_with(&["src/file.c"]);
        let loaded = load_text(&temp, &entry("src/file.c", "B", "MPL-2.0"));
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-tier-b")
        );
    }

    #[test]
    fn tier_b_under_addons_is_accepted() {
        let temp = root_with(&["addons/zfs/file.c"]);
        let loaded = load_text(&temp, &entry("addons/zfs/file.c", "B", "MPL-2.0"));
        assert!(
            !loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-tier-b")
        );
    }

    #[test]
    fn tier_b_under_third_party_is_accepted() {
        let temp = root_with(&["crates/third-party-foo/src/lib.rs"]);
        let loaded = load_text(
            &temp,
            &entry("crates/third-party-foo/src/lib.rs", "B", "MPL-2.0"),
        );
        assert!(
            !loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-tier-b")
        );
    }

    #[test]
    fn invalid_tier_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let loaded = load_text(&temp, &entry("crates/foo/src/lib.rs", "D", "MIT"));
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("tier"))
        );
    }

    #[test]
    fn invalid_license_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let loaded = load_text(&temp, &entry("crates/foo/src/lib.rs", "A", "MIT AND"));
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("license"))
        );
    }

    #[test]
    fn empty_required_field_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = entry("crates/foo/src/lib.rs", "A", "MIT")
            .replace("upstream = \"redox-kernel\"", "upstream = \"\"");
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema" && v.message.contains("upstream"))
        );
    }

    #[test]
    fn missing_path_is_rejected() {
        let temp = root_with(&[]);
        let loaded = load_text(&temp, &entry("crates/foo/src/lib.rs", "A", "MIT"));
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-path")
        );
    }

    #[test]
    fn absolute_path_is_rejected() {
        let temp = root_with(&[]);
        let loaded = load_text(&temp, &entry("/etc/passwd", "A", "MIT"));
        assert!(loaded
            .violations
            .iter()
            .any(|v| v.rule == "provenance-schema" && v.message.contains("repository-relative")));
    }

    #[test]
    fn duplicate_path_is_rejected() {
        let temp = root_with(&["crates/foo/src/lib.rs"]);
        let text = format!(
            "{}{}",
            entry("crates/foo/src/lib.rs", "A", "MIT"),
            entry("crates/foo/src/lib.rs", "A", "MIT")
        );
        let loaded = load_text(&temp, &text);
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-duplicate")
        );
    }

    #[test]
    fn import_must_be_an_array() {
        let temp = root_with(&[]);
        let loaded = load_text(&temp, "import = \"nope\"\n");
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.rule == "provenance-schema")
        );
        assert!(!loaded.usable);
    }

    #[test]
    fn missing_provenance_file_is_a_violation() {
        let temp = root_with(&[]);
        let loaded = load(temp.path());
        assert!(
            loaded
                .violations
                .iter()
                .any(|v| v.path == "PROVENANCE.toml")
        );
    }

    #[test]
    fn comments_only_file_is_valid() {
        let temp = root_with(&[]);
        let loaded = load_text(&temp, "# no imports yet\n");
        assert!(loaded.violations.is_empty());
        assert!(loaded.imports.is_empty());
    }

    #[test]
    fn longest_prefix_wins() {
        let imports = vec![
            Import {
                path: "crates/foo".to_string(),
                upstream: "a".to_string(),
                upstream_path: "a".to_string(),
                upstream_rev: SHA.to_string(),
                license: "MIT".to_string(),
                tier: "A".to_string(),
                copied: "2026-11-09".to_string(),
                modified: false,
                notes: None,
            },
            Import {
                path: "crates/foo/src/lib.rs".to_string(),
                upstream: "b".to_string(),
                upstream_path: "b".to_string(),
                upstream_rev: SHA.to_string(),
                license: "MIT".to_string(),
                tier: "A".to_string(),
                copied: "2026-11-09".to_string(),
                modified: false,
                notes: None,
            },
        ];
        let found = find_entry(&imports, "crates/foo/src/lib.rs").unwrap();
        assert_eq!(found.upstream, "b");
    }

    #[test]
    fn entry_prefix_respects_component_boundary() {
        let imports = vec![Import {
            path: "crates/foo/src".to_string(),
            upstream: "a".to_string(),
            upstream_path: "a".to_string(),
            upstream_rev: SHA.to_string(),
            license: "MIT".to_string(),
            tier: "A".to_string(),
            copied: "2026-11-09".to_string(),
            modified: false,
            notes: None,
        }];
        assert!(find_entry(&imports, "crates/foo/src/lib.rs").is_some());
        assert!(find_entry(&imports, "crates/foo/srcfoo/lib.rs").is_none());
    }

    #[test]
    fn notice_accepts_copyright_or_matching_spdx() {
        assert!(has_notice("// Copyright 2020 Someone\n", "MIT"));
        assert!(has_notice("// SPDX-License-Identifier: MIT\n", "MIT"));
        assert!(has_notice(
            "// SPDX-License-Identifier: MIT\n",
            "MIT OR Apache-2.0"
        ));
        assert!(!has_notice("// SPDX-License-Identifier: GPL-3.0\n", "MIT"));
        assert!(!has_notice("// nothing here\n", "MIT"));
    }

    #[test]
    fn notice_is_only_checked_in_first_ten_lines() {
        let content = format!("{}\n// Copyright\n", "\n".repeat(10));
        assert!(!has_notice(&content, "MIT"));
    }

    #[test]
    fn lock_parses_name_and_sha() {
        let lock = parse_lock("redox-kernel  abc123  A  https://example\n");
        assert_eq!(lock["redox-kernel"].sha, "abc123");
    }

    #[test]
    fn lock_missing_is_not_an_error() {
        let temp = TempDir::new("lock-missing");
        assert!(load_lock(&temp.path().join("nope.lock")).unwrap().is_none());
    }

    fn import_for_lock(upstream: &str, rev: &str) -> Import {
        Import {
            path: "crates/foo/src/lib.rs".to_string(),
            upstream: upstream.to_string(),
            upstream_path: "src/lib.rs".to_string(),
            upstream_rev: rev.to_string(),
            license: "MIT".to_string(),
            tier: "A".to_string(),
            copied: "2026-11-09".to_string(),
            modified: false,
            notes: None,
        }
    }

    #[test]
    fn lock_match_passes() {
        let lock = parse_lock("redox-kernel  0123456789abcdef0123456789abcdef01234567  A  u\n");
        let violations = check_lock(&[import_for_lock("redox-kernel", SHA)], &lock);
        assert!(violations.is_empty());
    }

    #[test]
    fn lock_revision_mismatch_fails() {
        let lock = parse_lock("redox-kernel  ffffffffffffffffffffffffffffffffffffffff  A  u\n");
        let violations = check_lock(&[import_for_lock("redox-kernel", SHA)], &lock);
        assert!(
            violations
                .iter()
                .any(|v| v.rule == "provenance-lock" && v.message.contains("does not match"))
        );
    }

    #[test]
    fn lock_unknown_upstream_fails() {
        let lock = parse_lock("other  0123456789abcdef0123456789abcdef01234567  A  u\n");
        let violations = check_lock(&[import_for_lock("redox-kernel", SHA)], &lock);
        assert!(
            violations
                .iter()
                .any(|v| v.rule == "provenance-lock" && v.message.contains("not listed"))
        );
    }

    #[test]
    fn lock_ignores_comments_and_short_lines() {
        let lock = parse_lock("# comment\nshort\nredox-kernel  abc  A  u\n");
        assert_eq!(lock.len(), 1);
    }
}
