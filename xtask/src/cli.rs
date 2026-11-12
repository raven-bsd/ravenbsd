// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hand-written command-line parser and dispatcher (CONTRACTS C1).

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};

use crate::build::{self, Arch, Component};
use crate::process::{CommandSpec, run as run_command};
use crate::{check, docs, guard, image, initfs, qtest, repo, run as run_image};

/// Stable `make help` text. The Makefile prints this through `cargo run -q -p xtask -- help`.
pub(crate) const HELP: &str = "\
RavenBSD build system

usage: make <target> [ARCH=x86_64|aarch64|riscv64] [PLATFORM=generic|pi4] [PROFILE=base] [RELEASE=1] [VERBOSE=1]

targets:
  help          show this list
  setup         install host packages, Rust toolchain and firmware paths
  doctor        report what is installed or missing
  check         fmt, clippy, host tests, stable guard, provenance and target clippy
  guard         stable-Rust guard only
  kernel        build the kernel ELF
  loader        build the UEFI loader
  dev           debug build of every component that exists
  release       smallest optimised build of every component that exists
  image         assemble a bootable image (ticket K05)
  run           run the image under QEMU (ticket K05)
  test          check plus QEMU integration tests
  world         build kernel, loader and userland (milestone M4)
  provenance    validate SPDX headers and PROVENANCE.toml
  toolchain     build the RavenBSD Rust toolchain (ticket K19)
  clean         remove target/ and build/
  distclean     also remove vendor/ and local build configuration

cargo xtask commands:
  check [--fast]
  guard
  build <kernel|loader|user> --arch <a> [--release] [-p <package>]...
  image   --arch <a> [--platform <generic|pi4>] [--profile <p>] [--cmdline <s>] [--initfs <p>] [--kernel <p>] [--output <path>]
  initfs  --arch <a> [--profile <p>]
  run     --arch <a> [--profile <p>] [--no-build] [-- <qemu args>]
  qtest   [--case <name> | --all] [--arch <a>] [--no-build]
  provenance
  docs
";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Guard,
    Check {
        fast: bool,
    },
    Build {
        component: Component,
        arch: Arch,
        release: bool,
        packages: Vec<String>,
    },
    Initfs {
        arch: Arch,
        profile: String,
    },
    Image {
        arch: Arch,
        platform: image::Platform,
        profile: String,
        cmdline: Option<String>,
        initfs: Option<String>,
        kernel: Option<String>,
        output: Option<String>,
    },
    Run {
        arch: Arch,
        profile: String,
        no_build: bool,
        extra: Vec<String>,
    },
    Qtest {
        args: Vec<String>,
    },
    Provenance,
    Docs,
}

/// Parse and execute a command line. Errors are usage or infrastructure errors and exit 2.
pub fn run_cli(args: Vec<String>) -> ExitCode {
    match parse_args(&args).and_then(execute) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: &[String]) -> Result<Command> {
    let Some(first) = args.first().map(String::as_str) else {
        return Ok(Command::Help);
    };
    let rest = &args[1..];
    match first {
        "-h" | "--help" | "help" => Ok(Command::Help),
        "guard" => {
            reject_extra(rest, "guard")?;
            Ok(Command::Guard)
        }
        "provenance" => {
            reject_extra(rest, "provenance")?;
            Ok(Command::Provenance)
        }
        "docs" => {
            reject_extra(rest, "docs")?;
            Ok(Command::Docs)
        }
        "check" => {
            let mut fast = false;
            for arg in rest {
                match arg.as_str() {
                    "--fast" => fast = true,
                    other => bail!("unknown check option `{other}`"),
                }
            }
            Ok(Command::Check { fast })
        }
        "build" => parse_build(rest),
        "initfs" => parse_initfs(rest),
        "image" => parse_image(rest),
        "run" => parse_run(rest),
        "qtest" => Ok(Command::Qtest {
            args: rest.to_vec(),
        }),
        other => bail!("unknown command `{other}` (try `help`)"),
    }
}

fn parse_build(args: &[String]) -> Result<Command> {
    let mut parser = Parser::new(args);
    let mut component = None;
    let mut arch = None;
    let mut release = false;
    let mut packages = Vec::new();
    while let Some(arg) = parser.next() {
        match arg.as_str() {
            "--arch" => arch = Some(parse_arch(&parser.value("--arch")?)?),
            "--release" => release = true,
            "-p" | "--package" => packages.push(parser.value("--package")?),
            other if other.starts_with('-') => bail!("unknown build option `{other}`"),
            other => {
                if component.is_some() {
                    bail!("unexpected build argument `{other}`");
                }
                component = Some(Component::parse(other).ok_or_else(|| {
                    anyhow!("unknown component `{other}` (expected kernel, loader or user)")
                })?);
            }
        }
    }
    let component =
        component.ok_or_else(|| anyhow!("build needs a component: kernel, loader or user"))?;
    let arch = arch.ok_or_else(|| anyhow!("--arch is required"))?;
    Ok(Command::Build {
        component,
        arch,
        release,
        packages,
    })
}

fn parse_initfs(args: &[String]) -> Result<Command> {
    let mut parser = Parser::new(args);
    let mut arch = None;
    let mut profile = "base".to_string();
    while let Some(arg) = parser.next() {
        match arg.as_str() {
            "--arch" => arch = Some(parse_arch(&parser.value("--arch")?)?),
            "--profile" => profile = parser.value("--profile")?,
            other => bail!("unknown initfs option `{other}`"),
        }
    }
    Ok(Command::Initfs {
        arch: arch.ok_or_else(|| anyhow!("--arch is required"))?,
        profile,
    })
}

fn parse_image(args: &[String]) -> Result<Command> {
    let mut parser = Parser::new(args);
    let mut arch = None;
    let mut platform = image::Platform::Generic;
    let mut profile = "base".to_string();
    let mut cmdline = None;
    let mut initfs = None;
    let mut kernel = None;
    let mut output = None;
    while let Some(arg) = parser.next() {
        match arg.as_str() {
            "--arch" => arch = Some(parse_arch(&parser.value("--arch")?)?),
            "--platform" => platform = image::Platform::parse(&parser.value("--platform")?)?,
            "--profile" => profile = parser.value("--profile")?,
            "--cmdline" => cmdline = Some(parser.value("--cmdline")?),
            "--initfs" => initfs = Some(parser.value("--initfs")?),
            "--kernel" => kernel = Some(parser.value("--kernel")?),
            "--output" => output = Some(parser.value("--output")?),
            other => bail!("unknown image option `{other}`"),
        }
    }
    Ok(Command::Image {
        arch: arch.ok_or_else(|| anyhow!("--arch is required"))?,
        platform,
        profile,
        cmdline,
        initfs,
        kernel,
        output,
    })
}

fn parse_run(args: &[String]) -> Result<Command> {
    let mut parser = Parser::new(args);
    let mut arch = None;
    let mut profile = "base".to_string();
    let mut no_build = false;
    let mut extra = Vec::new();
    while let Some(arg) = parser.next() {
        match arg.as_str() {
            "--arch" => arch = Some(parse_arch(&parser.value("--arch")?)?),
            "--profile" => profile = parser.value("--profile")?,
            "--no-build" => no_build = true,
            "--" => {
                extra = parser.rest();
                break;
            }
            other => bail!("unknown run option `{other}`"),
        }
    }
    Ok(Command::Run {
        arch: arch.ok_or_else(|| anyhow!("--arch is required"))?,
        profile,
        no_build,
        extra,
    })
}

fn parse_arch(value: &str) -> Result<Arch> {
    Arch::parse(value).ok_or_else(|| {
        anyhow!("unknown architecture `{value}` (expected x86_64, aarch64 or riscv64)")
    })
}

fn reject_extra(args: &[String], command: &str) -> Result<()> {
    if let Some(arg) = args.first() {
        bail!("{command} takes no arguments, got `{arg}`");
    }
    Ok(())
}

fn execute(command: Command) -> Result<u8> {
    match command {
        Command::Help => {
            print!("{HELP}");
            Ok(0)
        }
        Command::Guard => {
            let root = root()?;
            guard::run(&root)
        }
        Command::Check { fast } => {
            let root = root()?;
            let workspace = repo::Workspace::load(&root)?;
            check::run(&workspace, fast)
        }
        Command::Build {
            component,
            arch,
            release,
            packages,
        } => {
            let root = root()?;
            let workspace = repo::Workspace::load(&root)?;
            build::run(&workspace, component, arch, release, &packages)
        }
        Command::Initfs { arch, profile } => {
            let root = root()?;
            initfs::run(&root, arch, &profile)
        }
        Command::Image {
            arch,
            platform,
            profile,
            cmdline,
            initfs,
            kernel,
            output,
        } => {
            let root = root()?;
            image::run_with_overrides(
                &root,
                arch,
                &profile,
                cmdline.as_deref(),
                image::Overrides {
                    initfs: initfs.as_deref().map(Path::new),
                    kernel: kernel.as_deref().map(Path::new),
                    output: output.as_deref().map(Path::new),
                },
                platform,
            )
        }
        Command::Run {
            arch,
            profile,
            no_build,
            extra,
        } => {
            let root = root()?;
            run_image::run(&root, arch, &profile, no_build, &extra)
        }
        Command::Qtest { args } => {
            let root = root()?;
            qtest::run(&root, &args)
        }
        Command::Provenance => provenance(),
        Command::Docs => {
            let root = root()?;
            docs::run(&root)
        }
    }
}

fn root() -> Result<std::path::PathBuf> {
    let current = std::env::current_dir().map_err(|error| anyhow!("cannot get cwd: {error}"))?;
    repo::find_root(&current)
}

fn provenance() -> Result<u8> {
    let root = root()?;
    if !root.join("crates/raven-provenance/Cargo.toml").is_file() {
        bail!("component 'raven-provenance' does not exist yet (ticket B02)");
    }
    let spec = CommandSpec::new(
        "cargo",
        vec![
            "run".to_string(),
            "-q".to_string(),
            "-p".to_string(),
            "raven-provenance".to_string(),
            "--".to_string(),
            "check".to_string(),
            "--root".to_string(),
            ".".to_string(),
        ],
        &root,
    );
    let code = run_command(&spec)?;
    Ok(u8::from(code != 0))
}

struct Parser<'a> {
    args: &'a [String],
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(args: &'a [String]) -> Self {
        Self { args, index: 0 }
    }

    fn next(&mut self) -> Option<&'a String> {
        let value = self.args.get(self.index)?;
        self.index += 1;
        Some(value)
    }

    fn value(&mut self, flag: &str) -> Result<String> {
        self.next()
            .cloned()
            .ok_or_else(|| anyhow!("{flag} needs a value"))
    }

    fn rest(&mut self) -> Vec<String> {
        let rest = self.args.get(self.index..).unwrap_or(&[]).to_vec();
        self.index = self.args.len();
        rest
    }
}

#[cfg(test)]
#[path = "cli_ut.rs"]
mod tests;
