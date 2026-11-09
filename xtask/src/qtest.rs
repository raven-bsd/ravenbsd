// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Forwarding to the `raven-qtest` harness (ticket B01).
//!
//! CONTRACTS C1 spells the selector as `--case <name>` or `--all`; the harness CLI spells
//! them as the `run --case <file>` and `all` subcommands. This module translates one into the
//! other and resolves a bare case name to `tests/qtest/cases/<name>.toml`.

use std::path::Path;

use anyhow::{Result, anyhow, bail};

use crate::process::{CommandSpec, run as run_command};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Selection {
    All,
    Case(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QtestOptions {
    selection: Selection,
    arch: Option<String>,
    no_build: bool,
}

impl QtestOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut selection = None;
        let mut arch = None;
        let mut no_build = false;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--all" => set_selection(&mut selection, Selection::All)?,
                "--case" => {
                    index += 1;
                    let name = args
                        .get(index)
                        .ok_or_else(|| anyhow!("qtest: --case needs a case name"))?;
                    set_selection(&mut selection, Selection::Case(name.clone()))?;
                }
                "--arch" => {
                    index += 1;
                    arch = Some(
                        args.get(index)
                            .ok_or_else(|| anyhow!("qtest: --arch needs a value"))?
                            .clone(),
                    );
                }
                "--no-build" => no_build = true,
                other => bail!("qtest: unexpected argument `{other}`"),
            }
            index += 1;
        }
        let selection = selection
            .ok_or_else(|| anyhow!("qtest: exactly one of --all or --case <name> is required"))?;
        Ok(Self {
            selection,
            arch,
            no_build,
        })
    }

    fn harness_args(&self, cases_dir: &Path) -> Result<Vec<String>> {
        let mut args = match &self.selection {
            Selection::All => vec!["all".to_string()],
            Selection::Case(name) => vec![
                "run".to_string(),
                "--case".to_string(),
                resolve_case(name, cases_dir)?,
            ],
        };
        if let Some(arch) = &self.arch {
            args.push("--arch".to_string());
            args.push(arch.clone());
        }
        if self.no_build {
            args.push("--no-build".to_string());
        }
        Ok(args)
    }
}

fn set_selection(slot: &mut Option<Selection>, selection: Selection) -> Result<()> {
    if slot.is_some() {
        bail!("qtest: --all and --case are mutually exclusive");
    }
    *slot = Some(selection);
    Ok(())
}

fn resolve_case(name: &str, cases_dir: &Path) -> Result<String> {
    if Path::new(name).is_file() {
        return Ok(name.to_string());
    }
    if !name.contains('/') {
        let direct = cases_dir.join(name);
        if direct.is_file() {
            return Ok(direct.display().to_string());
        }
        let with_extension = cases_dir.join(format!("{name}.toml"));
        if with_extension.is_file() {
            return Ok(with_extension.display().to_string());
        }
    }
    bail!(
        "qtest: case `{name}` not found (also looked under {})",
        cases_dir.display()
    )
}

/// Forward the translated invocation to `cargo run -q -p raven-qtest -- ...`.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    if !root.join("tests/qtest/Cargo.toml").is_file() {
        bail!("qtest: tests/qtest does not exist yet (ticket B01)");
    }
    let options = QtestOptions::parse(args)?;
    let harness = options.harness_args(&root.join("tests/qtest/cases"))?;
    let mut command_args = vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "raven-qtest".to_string(),
        "--".to_string(),
    ];
    command_args.extend(harness);
    let code = run_command(&CommandSpec::new("cargo", command_args, root))?;
    Ok(u8::from(code != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_cases(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xtask-qtest-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn options(items: &[&str]) -> QtestOptions {
        QtestOptions::parse(
            &items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }

    #[test]
    fn all_is_translated_to_the_subcommand() {
        let cases = temp_cases("all");
        let args = options(&["--all", "--arch", "x86_64", "--no-build"])
            .harness_args(&cases)
            .unwrap();
        assert_eq!(args, vec!["all", "--arch", "x86_64", "--no-build"]);
    }

    #[test]
    fn all_without_flags_stays_minimal() {
        let cases = temp_cases("all-minimal");
        let args = options(&["--all"]).harness_args(&cases).unwrap();
        assert_eq!(args, vec!["all"]);
    }

    #[test]
    fn bare_case_name_resolves_to_a_case_file() {
        let cases = temp_cases("case-name");
        fs::write(cases.join("boot-uname.toml"), "").unwrap();
        let args = options(&["--case", "boot-uname", "--arch", "aarch64"])
            .harness_args(&cases)
            .unwrap();
        let expected = cases.join("boot-uname.toml").display().to_string();
        assert_eq!(
            args,
            vec!["run", "--case", expected.as_str(), "--arch", "aarch64"]
        );
    }

    #[test]
    fn case_file_name_is_accepted() {
        let cases = temp_cases("case-file-name");
        fs::write(cases.join("boot-uname.toml"), "").unwrap();
        let args = options(&["--case", "boot-uname.toml"])
            .harness_args(&cases)
            .unwrap();
        assert_eq!(args[2], cases.join("boot-uname.toml").display().to_string());
    }

    #[test]
    fn explicit_case_path_is_kept() {
        let cases = temp_cases("case-path");
        let path = cases.join("custom.toml");
        fs::write(&path, "").unwrap();
        let path_text = path.display().to_string();
        let args = options(&["--case", path_text.as_str()])
            .harness_args(&cases)
            .unwrap();
        assert_eq!(args, vec!["run", "--case", path_text.as_str()]);
    }

    #[test]
    fn missing_case_is_an_error() {
        let cases = temp_cases("case-missing");
        let error = options(&["--case", "nope"])
            .harness_args(&cases)
            .unwrap_err();
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn all_and_case_are_mutually_exclusive() {
        let args = vec!["--all".to_string(), "--case".to_string(), "x".to_string()];
        assert!(QtestOptions::parse(&args).is_err());
    }

    #[test]
    fn a_selector_is_required() {
        assert!(QtestOptions::parse(&[]).is_err());
        assert!(QtestOptions::parse(&["--arch".to_string(), "x86_64".to_string()]).is_err());
    }

    #[test]
    fn unknown_option_is_rejected() {
        assert!(QtestOptions::parse(&["--all".to_string(), "--wat".to_string()]).is_err());
    }

    #[test]
    fn values_are_required() {
        assert!(QtestOptions::parse(&["--case".to_string()]).is_err());
        assert!(QtestOptions::parse(&["--all".to_string(), "--arch".to_string()]).is_err());
    }

    #[test]
    fn no_build_is_forwarded_for_a_single_case() {
        let cases = temp_cases("case-no-build");
        fs::write(cases.join("one.toml"), "").unwrap();
        let args = options(&["--case", "one", "--no-build"])
            .harness_args(&cases)
            .unwrap();
        let expected = cases.join("one.toml").display().to_string();
        assert_eq!(args, vec!["run", "--case", expected.as_str(), "--no-build"]);
    }
}
