// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The five implemented builders: command vectors only, no process spawn here (section 4.5).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::model::linux_triple;
use crate::world::Package;

/// `--host` spelling for `configure`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostAlias {
    /// `x86_64-unknown-ravenbsd`.
    Raven,
    /// `x86_64-unknown-linux-gnu` (configure only; the compiler triple stays Raven).
    Linux,
}

impl HostAlias {
    /// Parse the CLI value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "raven" => Some(Self::Raven),
            "linux" => Some(Self::Linux),
            _ => None,
        }
    }

    /// The wire text.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Raven => "raven",
            Self::Linux => "linux",
        }
    }
}

/// Everything a builder needs to form its command vectors.
#[derive(Clone, Debug)]
pub struct BuildContext {
    /// Target architecture.
    pub arch: String,
    /// Compiler triple.
    pub triple: String,
    /// Host triple of the build machine (`clang -dumpmachine`).
    pub build_triple: String,
    /// Generated wrapper path.
    pub wrapper: PathBuf,
    /// `llvm-ar`.
    pub ar: String,
    /// `llvm-ranlib`.
    pub ranlib: String,
    /// `make`.
    pub make: String,
    /// Host `cc` for Kconfig tools.
    pub host_cc: String,
    /// Sysroot.
    pub sysroot: PathBuf,
    /// CMake toolchain file.
    pub toolchain_cmake: PathBuf,
    /// Host alias.
    pub host_alias: HostAlias,
}

/// One command vector of the pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedStep {
    /// Step name.
    pub name: String,
    /// Program.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Working directory.
    pub cwd: PathBuf,
    /// Step-specific environment (applied after the common environment).
    pub env: Vec<(String, String)>,
    /// Feed `/dev/null` to the process (Kconfig `oldconfig`).
    pub stdin_null: bool,
}

impl PlannedStep {
    fn new(
        name: &str,
        program: impl Into<String>,
        args: Vec<String>,
        cwd: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.to_string(),
            program: program.into(),
            args,
            cwd: cwd.into(),
            env: Vec::new(),
            stdin_null: true,
        }
    }

    fn with_env(mut self, key: &str, value: impl Into<String>) -> Self {
        self.env.push((key.to_string(), value.into()));
        self
    }
}

/// The host spelling of `configure`.
#[must_use]
pub fn host_spec(arch: &str, alias: HostAlias) -> String {
    match alias {
        HostAlias::Raven => format!("{arch}-unknown-ravenbsd"),
        HostAlias::Linux => {
            linux_triple(arch).unwrap_or_else(|| format!("{arch}-unknown-linux-gnu"))
        }
    }
}

/// Plan the steps of one package.
pub fn plan(
    kind: &str,
    package: &Package,
    ctx: &BuildContext,
    src: &Path,
    build: &Path,
    dest: &Path,
) -> Result<Vec<PlannedStep>> {
    let jobs = package.jobs_make;
    match kind {
        "ac" => Ok(autotools(package, ctx, src, dest, jobs)),
        "cf" => Ok(custom_configure(package, ctx, src, dest, jobs)),
        "mk" => Ok(make_only(package, ctx, src, dest, jobs)),
        "cm" => Ok(cmake(package, ctx, src, build, dest)),
        "kc" => Ok(kconfig(package, ctx, src, jobs)),
        other => bail!("builder-not-implemented:{other}"),
    }
}

fn autotools(
    package: &Package,
    ctx: &BuildContext,
    src: &Path,
    dest: &Path,
    jobs: u32,
) -> Vec<PlannedStep> {
    let configure = src.join("configure");
    let mut args = vec![
        format!("--build={}", ctx.build_triple),
        format!("--host={}", host_spec(&ctx.arch, ctx.host_alias)),
        "--prefix=/usr".to_string(),
        "--sysconfdir=/etc".to_string(),
        "--localstatedir=/var".to_string(),
        "--disable-dependency-tracking".to_string(),
    ];
    args.extend(package.configure.iter().cloned());
    let mut steps = vec![
        PlannedStep::new("configure", configure.display().to_string(), args, src),
        PlannedStep::new(
            "make",
            ctx.make.clone(),
            {
                let mut args = vec![format!("-j{jobs}")];
                args.extend(package.make_args.iter().cloned());
                args
            },
            src,
        ),
    ];
    steps.push(PlannedStep::new(
        "install",
        ctx.make.clone(),
        vec!["install".to_string(), format!("DESTDIR={}", dest.display())],
        src,
    ));
    steps
}

fn custom_configure(
    package: &Package,
    ctx: &BuildContext,
    src: &Path,
    dest: &Path,
    jobs: u32,
) -> Vec<PlannedStep> {
    let configure = src.join("configure");
    let mut args = vec!["--prefix=/usr".to_string()];
    args.extend(package.configure.iter().cloned());
    let mut steps = vec![
        PlannedStep::new("configure", configure.display().to_string(), args, src)
            .with_env("CC", ctx.wrapper.display().to_string())
            .with_env("AR", ctx.ar.clone()),
        PlannedStep::new(
            "make",
            ctx.make.clone(),
            {
                let mut args = vec![format!("-j{jobs}")];
                args.extend(package.make_args.iter().cloned());
                args
            },
            src,
        ),
    ];
    if package.install {
        steps.push(PlannedStep::new(
            "install",
            ctx.make.clone(),
            vec![
                "install".to_string(),
                format!("DESTDIR={}", dest.display()),
                "PREFIX=/usr".to_string(),
            ],
            src,
        ));
    }
    steps
}

fn make_only(
    package: &Package,
    ctx: &BuildContext,
    src: &Path,
    dest: &Path,
    jobs: u32,
) -> Vec<PlannedStep> {
    let mut args = vec![
        format!("-j{jobs}"),
        format!("CC={}", ctx.wrapper.display()),
        format!("AR={}", ctx.ar),
        format!("RANLIB={}", ctx.ranlib),
    ];
    args.extend(package.make_args.iter().cloned());
    let mut steps = vec![PlannedStep::new("make", ctx.make.clone(), args, src)];
    if package.install {
        steps.push(PlannedStep::new(
            "install",
            ctx.make.clone(),
            vec![
                "install".to_string(),
                format!("DESTDIR={}", dest.display()),
                "PREFIX=/usr".to_string(),
            ],
            src,
        ));
    }
    steps
}

fn cmake(
    package: &Package,
    ctx: &BuildContext,
    src: &Path,
    build: &Path,
    dest: &Path,
) -> Vec<PlannedStep> {
    let build = build.to_path_buf();
    let mut args = vec![
        "-S".to_string(),
        src.display().to_string(),
        "-B".to_string(),
        build.display().to_string(),
        "-G".to_string(),
        "Ninja".to_string(),
        format!("-DCMAKE_TOOLCHAIN_FILE={}", ctx.toolchain_cmake.display()),
        "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
        "-DCMAKE_BUILD_TYPE=Release".to_string(),
    ];
    args.extend(package.configure.iter().cloned());
    vec![
        PlannedStep::new("configure", "cmake", args, src),
        PlannedStep::new(
            "build",
            "cmake",
            vec!["--build".to_string(), build.display().to_string()],
            src,
        ),
        PlannedStep::new(
            "install",
            "cmake",
            vec!["--install".to_string(), build.display().to_string()],
            src,
        )
        .with_env("DESTDIR", dest.display().to_string()),
    ]
}

fn kconfig(package: &Package, ctx: &BuildContext, src: &Path, jobs: u32) -> Vec<PlannedStep> {
    let mut steps = vec![PlannedStep::new(
        "configure",
        ctx.make.clone(),
        vec![package.config_target.clone()],
        src,
    )];
    if !package.config_lines.is_empty() {
        steps.push(PlannedStep::new(
            "oldconfig",
            ctx.make.clone(),
            vec!["oldconfig".to_string()],
            src,
        ));
    }
    let mut args = vec![
        format!("-j{jobs}"),
        format!("CC={}", ctx.wrapper.display()),
        format!("HOSTCC={}", ctx.host_cc),
    ];
    args.extend(package.make_args.iter().cloned());
    steps.push(PlannedStep::new("make", ctx.make.clone(), args, src));
    steps
}

/// Append `config_lines` to `src/.config` before `make oldconfig`.
pub fn apply_config_lines(src: &Path, lines: &[String]) -> Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let path = src.join(".config");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(&path, text)?;
    Ok(())
}

#[cfg(test)]
#[path = "builders_ut.rs"]
mod tests;
