// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Initial filesystem packing and profile-driven user builds.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use raven_initfs::InitfsBuilder;

use crate::build::{self, Arch, Component};
use crate::profiles::Profile;
use crate::repo::Workspace;

/// Build `build/<arch>/initfs.img` from the profile's programs.
pub(crate) fn run(root: &Path, arch: Arch, profile: &str) -> Result<u8> {
    let path = build(root, arch, profile)?;
    println!("{}", path.display());
    Ok(0)
}

/// Build the initfs and return its path.
pub(crate) fn build(root: &Path, arch: Arch, profile_name: &str) -> Result<PathBuf> {
    let profile = Profile::load(root, profile_name)?;
    let workspace = Workspace::load(root)?;
    let packages: Vec<String> = profile
        .initfs
        .programs
        .iter()
        .map(|program| program.package.clone())
        .collect();
    if !packages.is_empty() {
        let code = build::run(&workspace, Component::User, arch, false, &packages)?;
        if code != 0 {
            bail!("user program build failed with exit code {code}");
        }
    }

    let mut builder = InitfsBuilder::new();
    for program in &profile.initfs.programs {
        let path = build::artifact_path(root, arch, Component::User, &program.package);
        let data = fs::read(&path).with_context(|| {
            format!(
                "cannot read built user program {} (package `{}` missing from the user build?)",
                path.display(),
                program.package
            )
        })?;
        builder.add_file(&program.path, &data, 0o755)?;
    }
    let image = builder.build();
    let output = root.join("build").join(arch.as_str()).join("initfs.img");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(&output, image).with_context(|| format!("cannot write {}", output.display()))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use raven_initfs::Initfs;

    #[test]
    fn packed_image_round_trips_through_the_reader() {
        let mut builder = InitfsBuilder::new();
        builder.add_file("/init", b"init-binary", 0o755).unwrap();
        builder
            .add_file("/bin/uname", b"uname-binary", 0o755)
            .unwrap();
        let image = builder.build();
        let parsed = Initfs::parse(&image).unwrap();
        assert_eq!(parsed.len(), 2);
        let init = parsed.get("/init").unwrap();
        assert_eq!(init.data, b"init-binary");
        assert_eq!(init.mode, 0o755);
        let uname = parsed.get("/bin/uname").unwrap();
        assert_eq!(uname.data, b"uname-binary");
    }
}
