// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Orchestration of all provenance rules over a repository tree.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::deps;
use crate::discover;
use crate::ignore;
use crate::model::{CheckOptions, CheckReport, Violation};
use crate::provenance;
use crate::spdx;

/// Run every provenance rule against `options.root`.
///
/// Returns a report; a non-empty `violations` list is the caller's signal to exit 1.
pub fn check(options: &CheckOptions) -> Result<CheckReport> {
    let root = options.root.as_path();
    if !root.is_dir() {
        bail!("root is not a directory: {}", root.display());
    }
    let ignored = ignore::load(root);
    let files: Vec<String> = discover::discover(root)?
        .into_iter()
        .filter(|file| !ignore::is_ignored(file, &ignored))
        .collect();
    let mut report = CheckReport {
        violations: Vec::new(),
        warnings: Vec::new(),
        files_checked: files.len(),
    };

    let loaded = provenance::load(root);
    report.violations.extend(loaded.violations.iter().cloned());
    let lock_path = lock_path(root, options.donors_lock.as_deref());
    match provenance::load_lock(&lock_path) {
        Ok(Some(lock)) => report
            .violations
            .extend(provenance::check_lock(&loaded.imports, &lock)),
        Ok(None) => report.warnings.push(format!(
            "donors lock not found at {}; revision check skipped",
            lock_path.display()
        )),
        Err(error) => report.warnings.push(format!(
            "cannot read donors lock at {}: {error}; revision check skipped",
            lock_path.display()
        )),
    }

    for file in &files {
        check_file(root, file, &loaded.imports, &mut report);
    }

    let (dependency_violations, dependency_warnings) = deps::check(root);
    report.violations.extend(dependency_violations);
    report.warnings.extend(dependency_warnings);
    report.violations.sort();
    report.warnings.sort();
    report.warnings.dedup();
    Ok(report)
}

fn check_file(root: &Path, file: &str, imports: &[provenance::Import], report: &mut CheckReport) {
    let content = match fs::read_to_string(root.join(file)) {
        Ok(content) => content,
        Err(error) => {
            report.violations.push(Violation::new(
                file,
                "io",
                format!("cannot read file: {error}"),
            ));
            return;
        }
    };
    let entry = provenance::find_entry(imports, file);
    if spdx::is_checked_file(file) {
        match spdx::find_spdx(&content, 5) {
            None => report.violations.push(Violation::new(
                file,
                "spdx-missing",
                "missing SPDX-License-Identifier in the first 5 lines",
            )),
            Some(expression) => {
                if let Some(entry) = entry {
                    if !spdx::expressions_compatible(&expression, &entry.license) {
                        report.violations.push(Violation::new(
                            file,
                            "provenance-license",
                            format!(
                                "SPDX expression `{expression}` is incompatible with the \
                                 import licence `{}`",
                                entry.license
                            ),
                        ));
                    }
                } else if !spdx::is_project_license(&expression) {
                    report.violations.push(Violation::new(
                        file,
                        "spdx-license",
                        format!("first-party file must use BSD-3-Clause, found `{expression}`"),
                    ));
                }
            }
        }
        if entry.is_none() && !spdx::has_copyright(&content, 5) {
            report.violations.push(Violation::new(
                file,
                "spdx-copyright",
                format!(
                    "missing copyright line `{}` in the first 5 lines",
                    spdx::COPYRIGHT_LINE
                ),
            ));
        }
    } else if let Some(entry) = entry
        && let Some(expression) = spdx::find_spdx(&content, 10)
        && !spdx::expressions_compatible(&expression, &entry.license)
    {
        report.violations.push(Violation::new(
            file,
            "provenance-license",
            format!(
                "SPDX expression `{expression}` is incompatible with the import \
                 licence `{}`",
                entry.license
            ),
        ));
    }
    if let Some(entry) = entry
        && !provenance::has_notice(&content, &entry.license)
    {
        report.violations.push(Violation::new(
            file,
            "provenance-notice",
            "the first 10 lines carry no copyright/licence notice for this import",
        ));
    }
}

fn lock_path(root: &Path, explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Some(path) = std::env::var_os("RAVEN_DONORS_LOCK") {
        return PathBuf::from(path);
    }
    root.join("../ravenbsd-resources/donors.lock")
}

/// List every checked file with the licence that applies to it.
pub fn list(root: &Path) -> Result<Vec<(String, String)>> {
    if !root.is_dir() {
        bail!("root is not a directory: {}", root.display());
    }
    let ignored = ignore::load(root);
    let loaded = provenance::load(root);
    let mut entries = Vec::new();
    for file in discover::discover(root)? {
        if ignore::is_ignored(&file, &ignored) {
            continue;
        }
        let import = provenance::find_entry(&loaded.imports, &file);
        if !spdx::is_checked_file(&file) && import.is_none() {
            continue;
        }
        let license = import.map_or_else(
            || spdx::PROJECT_LICENSE.to_string(),
            |entry| entry.license.clone(),
        );
        entries.push((file, license));
    }
    Ok(entries)
}

/// Insert the standard header into first-party files that lack it.
///
/// Returns the repository-relative paths that were updated.
pub fn fix_spdx(root: &Path) -> Result<Vec<String>> {
    if !root.is_dir() {
        bail!("root is not a directory: {}", root.display());
    }
    let ignored = ignore::load(root);
    let loaded = provenance::load(root);
    if !loaded.usable {
        bail!("cannot use PROVENANCE.toml; refusing to guess which files are imports");
    }
    let mut fixed = Vec::new();
    for file in discover::discover(root)? {
        if ignore::is_ignored(&file, &ignored)
            || !spdx::is_checked_file(&file)
            || provenance::find_entry(&loaded.imports, &file).is_some()
        {
            continue;
        }
        let path = root.join(&file);
        let content =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        if spdx::find_spdx(&content, 5).is_some() {
            continue;
        }
        let Some(updated) = spdx::fixed_content(&file, &content) else {
            continue;
        };
        fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
        fixed.push(file);
    }
    Ok(fixed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    const HEADER: &str = "// SPDX-License-Identifier: BSD-3-Clause\n\
                          // Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\n";
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const IGNORE: &str =
        "Cargo.lock\nLICENSE\nNOTICE\nPROVENANCE.toml\nAI_POLICY.md\nREADME.md\ndocs/man\n";

    fn repo() -> TempDir {
        let temp = TempDir::new("check");
        temp.write(".provenance-ignore", IGNORE);
        temp.write("PROVENANCE.toml", "# no imports\n");
        temp
    }

    fn run(temp: &TempDir) -> CheckReport {
        let mut options = CheckOptions::new(temp.path());
        options.donors_lock = Some(temp.path().join("no-such-lock"));
        check(&options).unwrap()
    }

    fn entry(path: &str, tier: &str, license: &str) -> String {
        format!(
            "[[import]]\npath = \"{path}\"\nupstream = \"redox-kernel\"\n\
             upstream_path = \"src/lib.rs\"\nupstream_rev = \"{SHA}\"\n\
             license = \"{license}\"\ntier = \"{tier}\"\ncopied = \"2026-11-09\"\n\
             modified = false\n"
        )
    }

    #[test]
    fn clean_repository_has_no_violations() {
        let temp = repo();
        temp.write("src/main.rs", &format!("{HEADER}fn main() {{}}\n"));
        let report = run(&temp);
        assert!(report.violations.is_empty(), "{:?}", report.violations);
        assert!(!report.warnings.is_empty(), "missing lock must warn");
    }

    #[test]
    fn missing_spdx_header_is_reported() {
        let temp = repo();
        temp.write("src/bad.rs", "fn main() {}\n");
        let report = run(&temp);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.path == "src/bad.rs" && v.rule == "spdx-missing")
        );
    }

    #[test]
    fn wrong_first_party_licence_is_reported() {
        let temp = repo();
        temp.write(
            "src/bad.rs",
            "// SPDX-License-Identifier: MIT\n\
             // Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n",
        );
        let report = run(&temp);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.rule == "spdx-license" && v.message.contains("MIT"))
        );
    }

    #[test]
    fn missing_copyright_is_reported() {
        let temp = repo();
        temp.write(
            "src/bad.rs",
            "// SPDX-License-Identifier: BSD-3-Clause\n\
             // Copyright 2020 Someone Else\n",
        );
        let report = run(&temp);
        assert!(report.violations.iter().any(|v| v.rule == "spdx-copyright"));
    }

    #[test]
    fn imported_file_needs_a_notice() {
        let temp = repo();
        temp.write("addons/foo/file.c", "int x;\n");
        temp.write("PROVENANCE.toml", &entry("addons/foo/file.c", "A", "MIT"));
        let report = run(&temp);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.rule == "provenance-notice")
        );
    }

    #[test]
    fn imported_file_licence_must_match_the_entry() {
        let temp = repo();
        temp.write(
            "addons/foo/file.c",
            "// SPDX-License-Identifier: MIT\n/* Copyright 2020 Upstream */\n",
        );
        temp.write(
            "PROVENANCE.toml",
            &entry("addons/foo/file.c", "A", "Apache-2.0"),
        );
        let report = run(&temp);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.rule == "provenance-license")
        );
    }

    #[test]
    fn tier_c_import_is_reported() {
        let temp = repo();
        temp.write("addons/foo/file.c", "/* Copyright 2020 Upstream */\n");
        temp.write(
            "PROVENANCE.toml",
            &entry("addons/foo/file.c", "C", "GPL-2.0"),
        );
        let report = run(&temp);
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.rule == "provenance-tier-c")
        );
    }

    #[test]
    fn fix_spdx_then_check_passes() {
        let temp = repo();
        temp.write("src/main.rs", "fn main() {}\n");
        temp.write("scripts/run.sh", "#!/bin/sh\necho ok\n");
        temp.write("docs/design.md", "# Design\n");
        temp.write("Makefile", "all:\n\t@true\n");
        temp.write("kernel/linker/x86_64.ld", "SECTIONS {}\n");
        let fixed = fix_spdx(temp.path()).unwrap();
        assert_eq!(fixed.len(), 5, "{fixed:?}");
        assert!(temp.read("scripts/run.sh").starts_with("#!/bin/sh\n# SPDX"));
        assert!(temp.read("docs/design.md").starts_with("<!-- SPDX"));
        assert!(temp.read("kernel/linker/x86_64.ld").starts_with("/* SPDX"));
        assert!(fix_spdx(temp.path()).unwrap().is_empty());
        let report = run(&temp);
        assert!(report.violations.is_empty(), "{:?}", report.violations);
    }

    #[test]
    fn fix_spdx_skips_imported_files() {
        let temp = repo();
        temp.write("addons/foo/lib.rs", "fn x() {}\n");
        temp.write("PROVENANCE.toml", &entry("addons/foo/lib.rs", "A", "MIT"));
        let fixed = fix_spdx(temp.path()).unwrap();
        assert!(fixed.is_empty());
        assert_eq!(temp.read("addons/foo/lib.rs"), "fn x() {}\n");
    }

    #[test]
    fn list_reports_effective_licences() {
        let temp = repo();
        temp.write("src/main.rs", &format!("{HEADER}fn main() {{}}\n"));
        temp.write(
            "addons/foo/file.c",
            "// SPDX-License-Identifier: MIT\n/* Copyright 2020 Upstream */\n",
        );
        temp.write("PROVENANCE.toml", &entry("addons/foo/file.c", "A", "MIT"));
        let entries = list(temp.path()).unwrap();
        assert!(entries.contains(&("src/main.rs".to_string(), "BSD-3-Clause".to_string())));
        assert!(entries.contains(&("addons/foo/file.c".to_string(), "MIT".to_string())));
    }

    #[test]
    fn check_does_not_modify_files() {
        let temp = repo();
        let content = "fn main() {}\n";
        temp.write("src/bad.rs", content);
        let _ = run(&temp);
        assert_eq!(temp.read("src/bad.rs"), content);
    }
}
