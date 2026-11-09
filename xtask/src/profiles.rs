// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Profile parsing (`profiles/<name>.toml`, CONTRACTS C1).
//!
//! Unknown keys are rejected: a typo in a profile must fail the build instead of being ignored.
//! The file name, the `name` field and the program paths are cross-checked.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// A build profile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub(crate) name: String,
    pub(crate) initfs: Initfs,
    pub(crate) boot: Boot,
}

/// The programs packed into the initial filesystem.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Initfs {
    pub(crate) programs: Vec<Program>,
}

/// One program entry in a profile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Program {
    pub(crate) package: String,
    pub(crate) path: String,
}

/// Boot parameters stored in the profile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Boot {
    pub(crate) cmdline: String,
    pub(crate) log_level: String,
}

impl Profile {
    /// Load `profiles/<name>.toml` under `root`.
    pub(crate) fn load(root: &Path, name: &str) -> Result<Self> {
        if !valid_name(name) {
            bail!("invalid profile name `{name}`");
        }
        let path = root.join("profiles").join(format!("{name}.toml"));
        let text = fs::read_to_string(&path)
            .with_context(|| format!("cannot read profile {}", path.display()))?;
        let profile: Self = toml::from_str(&text)
            .with_context(|| format!("cannot parse profile {}", path.display()))?;
        profile.validate(name)?;
        Ok(profile)
    }

    fn validate(&self, file_name: &str) -> Result<()> {
        if self.name != file_name {
            bail!(
                "profile file `{file_name}.toml` declares name `{}`",
                self.name
            );
        }
        if !matches!(
            self.boot.log_level.as_str(),
            "error" | "warn" | "info" | "debug"
        ) {
            bail!(
                "profile `{}` has invalid log_level `{}`",
                self.name,
                self.boot.log_level
            );
        }
        if self.boot.cmdline.contains('\0') {
            bail!("profile `{}` has a NUL in cmdline", self.name);
        }
        let mut paths = std::collections::BTreeSet::new();
        for program in &self.initfs.programs {
            if program.package.is_empty() {
                bail!(
                    "profile `{}` has a program with an empty package",
                    self.name
                );
            }
            if !program.path.starts_with('/') || program.path.len() < 2 {
                bail!(
                    "profile `{}` program `{}` has invalid path `{}`",
                    self.name,
                    program.package,
                    program.path
                );
            }
            if !paths.insert(program.path.as_str()) {
                bail!(
                    "profile `{}` contains duplicate path `{}`",
                    self.name,
                    program.path
                );
            }
        }
        Ok(())
    }
}

/// Load and validate every `profiles/*.toml` file. Used by the profile test matrix.
#[cfg(test)]
pub(crate) fn load_all(root: &Path) -> Result<Vec<Profile>> {
    let dir = root.join("profiles");
    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("cannot list profile directory {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("cannot read {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        names.push(stem.to_string());
    }
    names.sort();
    names.iter().map(|name| Profile::load(root, name)).collect()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
    }

    fn parse(text: &str, name: &str) -> Result<Profile> {
        let profile: Profile = toml::from_str(text)?;
        profile.validate(name)?;
        Ok(profile)
    }

    const GOOD: &str = r#"
name = "base"
[initfs]
programs = [
  { package = "raven-init", path = "/init" },
  { package = "raven-uname", path = "/bin/uname" },
]
[boot]
cmdline = "console=ttyS0 test-exit=1"
log_level = "info"
"#;

    #[test]
    fn every_profile_in_the_tree_parses() {
        let profiles = load_all(workspace_root()).unwrap();
        assert!(profiles.len() >= 3);
        let names: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.name.as_str())
            .collect();
        assert_eq!(names, vec!["base", "dev", "tiny"]);
    }

    #[test]
    fn base_profile_matches_contract_c1() {
        let profile = Profile::load(workspace_root(), "base").unwrap();
        let pairs: Vec<(&str, &str)> = profile
            .initfs
            .programs
            .iter()
            .map(|program| (program.package.as_str(), program.path.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("raven-init", "/init"),
                ("raven-uname", "/bin/uname"),
                ("raven-echo", "/bin/echo"),
            ]
        );
        assert_eq!(profile.boot.cmdline, "console=ttyS0 test-exit=1");
        assert_eq!(profile.boot.log_level, "info");
    }

    #[test]
    fn tiny_profile_has_only_init_and_uname() {
        let profile = Profile::load(workspace_root(), "tiny").unwrap();
        let packages: Vec<&str> = profile
            .initfs
            .programs
            .iter()
            .map(|program| program.package.as_str())
            .collect();
        assert_eq!(packages, vec!["raven-init", "raven-uname"]);
    }

    #[test]
    fn dev_profile_contains_the_base_programs() {
        let base = Profile::load(workspace_root(), "base").unwrap();
        let dev = Profile::load(workspace_root(), "dev").unwrap();
        assert_eq!(dev.initfs.programs, base.initfs.programs);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let text = GOOD.replace("[boot]", "[boot]\nextra = true");
        assert!(parse(&text, "base").is_err());
    }

    #[test]
    fn name_must_match_the_file_stem() {
        assert!(parse(GOOD, "tiny").is_err());
    }

    #[test]
    fn invalid_log_level_is_rejected() {
        let text = GOOD.replace("log_level = \"info\"", "log_level = \"chatty\"");
        assert!(parse(&text, "base").is_err());
    }

    #[test]
    fn duplicate_paths_are_rejected() {
        let text = GOOD.replace("/bin/uname", "/init");
        assert!(parse(&text, "base").is_err());
    }

    #[test]
    fn program_paths_must_be_absolute() {
        let text = GOOD.replace("/bin/uname", "bin/uname");
        assert!(parse(&text, "base").is_err());
    }

    #[test]
    fn profile_names_cannot_traverse_directories() {
        assert!(!valid_name("../base"));
        assert!(!valid_name("a/b"));
        assert!(valid_name("base"));
        assert!(valid_name("tiny-dev_1"));
    }

    #[test]
    fn missing_boot_section_is_rejected() {
        let text = GOOD.replace(
            "[boot]\ncmdline = \"console=ttyS0 test-exit=1\"\nlog_level = \"info\"\n",
            "",
        );
        assert!(parse(&text, "base").is_err());
    }
}
