// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Dependency licence allow-list, evaluated over `cargo metadata --offline`.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::model::Violation;
use crate::spdx;

/// Licences accepted for external crates.
pub(crate) const ALLOWED_LICENSES: &[&str] = &[
    "mit",
    "apache-2.0",
    "bsd-2-clause",
    "bsd-3-clause",
    "isc",
    "0bsd",
    "zlib",
    "unicode-3.0",
    "cc0-1.0",
    "unlicense",
];

/// Whether a licence expression is acceptable for a dependency.
pub(crate) fn license_allowed(expression: &str, allow_mpl: bool) -> bool {
    let expression = normalize_legacy_dependency_license(expression);
    let Ok(expression) = spdx::parse_expression(expression) else {
        return false;
    };
    let Ok(alternatives) = spdx::alternatives(&expression) else {
        return false;
    };
    alternatives
        .iter()
        .any(|alternative| alternative.iter().all(|term| term_allowed(term, allow_mpl)))
}

/// Map the one legacy Cargo licence spelling that is not valid SPDX.
///
/// `bit_field` 0.10.3 (a `uefi` dependency) declares `Apache-2.0/MIT`; Cargo
/// metadata reports that string verbatim. Only this exact expression is
/// translated, so unknown or malformed expressions still fail. This applies to
/// dependency metadata, not to SPDX source-header validation.
fn normalize_legacy_dependency_license(expression: &str) -> &str {
    match expression {
        "Apache-2.0/MIT" => "Apache-2.0 OR MIT",
        other => other,
    }
}

fn term_allowed(term: &str, allow_mpl: bool) -> bool {
    let base = term.split(" with ").next().unwrap_or(term);
    ALLOWED_LICENSES.contains(&base) || (allow_mpl && base == "mpl-2.0")
}

/// Read `.provenance-allow-mpl` (one crate name per line).
pub(crate) fn load_mpl_allow(root: &Path) -> HashSet<String> {
    let Ok(text) = fs::read_to_string(root.join(".provenance-allow-mpl")) else {
        return HashSet::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Run the dependency check over `root`; returns violations and non-fatal warnings.
pub(crate) fn check(root: &Path) -> (Vec<Violation>, Vec<String>) {
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    match cargo_metadata(root) {
        Ok(metadata) => {
            let allow_mpl = load_mpl_allow(root);
            let members: HashSet<&str> = metadata
                .get("workspace_members")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if let Some(packages) = metadata.get("packages").and_then(Value::as_array) {
                for package in packages {
                    let Some(id) = package.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    if members.contains(id) {
                        continue;
                    }
                    let name = package
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("<unknown>");
                    match package.get("license").and_then(Value::as_str) {
                        None => violations.push(Violation::new(
                            name,
                            "dependency-license",
                            format!("package `{name}` has no license field"),
                        )),
                        Some(expression)
                            if !license_allowed(expression, allow_mpl.contains(name)) =>
                        {
                            violations.push(Violation::new(
                                name,
                                "dependency-license",
                                format!(
                                    "package `{name}` uses licence expression `{expression}`, \
                                     which is not on the allow-list"
                                ),
                            ));
                        }
                        Some(_) => {}
                    }
                }
            } else {
                warnings.push(
                    "cargo metadata returned no packages list; dependency licence check skipped"
                        .to_string(),
                );
            }
        }
        Err(error) => warnings.push(format!(
            "cargo metadata --offline failed; dependency licence check skipped: {error}"
        )),
    }
    (violations, warnings)
}

fn cargo_metadata(root: &Path) -> Result<Value, String> {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--offline")
        .arg("--format-version")
        .arg("1")
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run cargo: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("unknown error")
            .trim();
        return Err(message.to_string());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid cargo metadata JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn permissive_expressions_are_allowed() {
        for expression in [
            "MIT",
            "Apache-2.0",
            "BSD-2-Clause",
            "BSD-3-Clause",
            "ISC",
            "0BSD",
            "Zlib",
            "Unicode-3.0",
            "CC0-1.0",
            "Unlicense",
            "MIT OR Apache-2.0",
            "Apache-2.0 OR MIT",
        ] {
            assert!(
                license_allowed(expression, false),
                "{expression} should pass"
            );
        }
    }

    #[test]
    fn copyleft_expressions_are_rejected() {
        for expression in [
            "GPL-3.0",
            "GPL-2.0-or-later",
            "AGPL-3.0",
            "LGPL-2.1",
            "MIT AND GPL-2.0",
        ] {
            assert!(
                !license_allowed(expression, false),
                "{expression} should fail"
            );
        }
    }

    #[test]
    fn allowed_or_alternative_rescues_copyleft() {
        assert!(license_allowed("MIT OR GPL-3.0", false));
        assert!(license_allowed("GPL-3.0 OR MIT", false));
        assert!(!license_allowed("MIT AND (GPL-2.0 OR AGPL-3.0)", false));
    }

    #[test]
    fn llvm_exception_uses_the_base_licence() {
        assert!(license_allowed("Apache-2.0 WITH LLVM-exception", false));
        assert!(!license_allowed(
            "GPL-2.0 WITH Classpath-exception-2.0",
            false
        ));
    }

    #[test]
    fn mpl_requires_an_approval_line() {
        assert!(!license_allowed("MPL-2.0", false));
        assert!(license_allowed("MPL-2.0", true));
    }

    #[test]
    fn mpl_or_permissive_is_allowed_without_approval() {
        assert!(license_allowed("MPL-2.0 OR MIT", false));
    }

    #[test]
    fn invalid_expressions_are_rejected() {
        assert!(!license_allowed("", false));
        assert!(!license_allowed("MIT AND", false));
        assert!(!license_allowed("Dont-Care", false));
    }

    #[test]
    fn allow_mpl_file_is_parsed() {
        let temp = TempDir::new("deps-mpl");
        temp.write(
            ".provenance-allow-mpl",
            "# approved\nsome-crate\n\nother-crate\n",
        );
        let allowed = load_mpl_allow(temp.path());
        assert!(allowed.contains("some-crate"));
        assert!(allowed.contains("other-crate"));
        assert_eq!(allowed.len(), 2);
    }

    #[test]
    fn missing_manifest_is_a_warning_not_a_violation() {
        let temp = TempDir::new("deps-no-manifest");
        let (violations, warnings) = check(temp.path());
        assert!(violations.is_empty());
        assert!(!warnings.is_empty());
    }

    #[test]
    fn cargo_metadata_is_offline() {
        // A project with a path dependency that does not exist fails offline quickly; the
        // important property is that `check` never surfaces a network fetch as success.
        let temp = TempDir::new("deps-offline");
        temp.write(
            "Cargo.toml",
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        temp.write("src/lib.rs", "");
        let (violations, warnings) = check(temp.path());
        assert!(violations.is_empty());
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    fn fixture_repo(
        label: &str,
        package: &str,
        version: &str,
        license: &str,
        allow: Option<&str>,
    ) -> TempDir {
        let temp = TempDir::new(label);
        temp.write(
            "Cargo.toml",
            &format!(
                "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                 [dependencies]\n{package} = {{ path = \"deps/{package}\" }}\n"
            ),
        );
        temp.write(
            &format!("deps/{package}/Cargo.toml"),
            &format!(
                "[package]\nname = \"{package}\"\nversion = \"{version}\"\n\
                 edition = \"2021\"\nlicense = \"{license}\"\n"
            ),
        );
        temp.write(&format!("deps/{package}/src/lib.rs"), "");
        temp.write("src/lib.rs", "");
        temp.write("PROVENANCE.toml", "# fixture\n");
        temp.write(
            ".provenance-ignore",
            "Cargo.lock\nCargo.toml\nPROVENANCE.toml\n",
        );
        if let Some(lines) = allow {
            temp.write(".provenance-allow-mpl", lines);
        }
        temp
    }

    fn dependency_violations(root: &Path) -> Vec<Violation> {
        let (violations, warnings) = check(root);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        violations
            .into_iter()
            .filter(|violation| violation.rule == "dependency-license")
            .collect()
    }

    #[test]
    fn legacy_apache_mit_slash_mapping_is_exact() {
        assert!(license_allowed("Apache-2.0/MIT", false));
        for expression in [
            "apache-2.0/mit",
            "Apache-2.0 / MIT",
            "MIT/Apache-2.0",
            "Apache-2.0/MIT OR MIT",
        ] {
            assert!(
                !license_allowed(expression, false),
                "{expression} must not be accepted by the exact mapping"
            );
        }
    }

    #[test]
    fn approved_ucs2_mpl_dependency_passes() {
        let temp = fixture_repo(
            "deps-ucs2",
            "ucs2",
            "0.3.3",
            "MPL-2.0",
            Some("# approved\nucs2\n"),
        );
        let violations = dependency_violations(temp.path());
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn mpl_dependency_without_approval_fails() {
        let temp = fixture_repo("deps-ucs2-noallow", "ucs2", "0.3.3", "MPL-2.0", None);
        let violations = dependency_violations(temp.path());
        assert!(violations.iter().any(|violation| violation.path == "ucs2"));
    }

    #[test]
    fn approval_is_scoped_to_the_named_crate() {
        let temp = fixture_repo(
            "deps-other-mpl",
            "other-crate",
            "0.1.0",
            "MPL-2.0",
            Some("ucs2\n"),
        );
        let violations = dependency_violations(temp.path());
        assert!(
            violations
                .iter()
                .any(|violation| violation.path == "other-crate")
        );
    }

    #[test]
    fn legacy_slash_metadata_passes_via_check() {
        let temp = fixture_repo(
            "deps-bit-field",
            "bit_field",
            "0.10.3",
            "Apache-2.0/MIT",
            None,
        );
        let violations = dependency_violations(temp.path());
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn unknown_dependency_expression_still_fails() {
        let temp = fixture_repo("deps-unknown", "bad-crate", "0.1.0", "FooBar-1.0", None);
        let violations = dependency_violations(temp.path());
        assert!(
            violations
                .iter()
                .any(|violation| violation.path == "bad-crate")
        );
    }

    #[test]
    fn gpl_dependency_still_fails() {
        let temp = fixture_repo("deps-gpl", "gpl-crate", "0.1.0", "GPL-3.0-only", None);
        let violations = dependency_violations(temp.path());
        assert!(
            violations
                .iter()
                .any(|violation| violation.path == "gpl-crate")
        );
    }

    #[test]
    fn missing_spdx_header_still_fails() {
        let temp = TempDir::new("deps-spdx");
        temp.write("PROVENANCE.toml", "# fixture\n");
        temp.write(
            ".provenance-ignore",
            "Cargo.lock\nCargo.toml\nPROVENANCE.toml\n",
        );
        temp.write("src/bad.rs", "fn main() {}\n");
        let report = crate::check::check(&crate::model::CheckOptions::new(temp.path())).unwrap();
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.path == "src/bad.rs"
                    && violation.rule == "spdx-missing"),
            "{:?}",
            report.violations
        );
    }

    #[test]
    fn legacy_slash_source_header_is_not_mapped() {
        let temp = TempDir::new("deps-spdx-slash");
        temp.write("PROVENANCE.toml", "# fixture\n");
        temp.write(
            ".provenance-ignore",
            "Cargo.lock\nCargo.toml\nPROVENANCE.toml\n",
        );
        temp.write(
            "src/bad.rs",
            "// SPDX-License-Identifier: Apache-2.0/MIT\n\
             // Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n",
        );
        let report = crate::check::check(&crate::model::CheckOptions::new(temp.path())).unwrap();
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.path == "src/bad.rs"
                    && violation.rule == "spdx-license"),
            "{:?}",
            report.violations
        );
    }

    #[test]
    fn repo_allow_file_names_ucs2() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let allowed = load_mpl_allow(&root);
        assert!(allowed.contains("ucs2"), "{allowed:?}");
    }
}
