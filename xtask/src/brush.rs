// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `build brush` component and the `shell-brush` qtest selector.
//!
//! brush lives in its own workspace under `userland/brush`, so it is not a
//! member of the root workspace and cannot use `build::run`. This module owns
//! the small amount of glue: the RavenBSD target triple, the static-PIE CRT and
//! libc link arguments, the artifact staging path and the qtest case alias.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::build::Arch;
use crate::process::{CommandSpec, run as run_command};

/// The package and binary built by this component.
const PACKAGE: &str = "brush-shell";
/// The binary name produced by the package.
const BINARY: &str = "brush";
/// Stable staging path used by the shell profile until brush becomes a normal
/// workspace user package. The x86-64 bring-up image is the only consumer.
const STAGED: &str = "build/brush";

/// The RavenBSD target triple for an architecture.
pub(crate) fn target_for(arch: Arch) -> &'static str {
    match arch {
        Arch::X86_64 => "x86_64-unknown-ravenbsd",
        Arch::Aarch64 => "aarch64-unknown-ravenbsd",
        Arch::Riscv64 => "riscv64gc-unknown-ravenbsd",
    }
}

/// Parsed `build brush` options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BuildOptions {
    arch: Arch,
    release: bool,
}

fn parse_build(args: &[String]) -> Result<BuildOptions> {
    let mut arch = None;
    // The target shell is installed in the boot image; a debug brush carries megabytes of DWARF
    // and no longer fits the ESP. Release is the default, as for the uutils target build.
    let mut release = true;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--arch" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| anyhow::anyhow!("build brush: --arch needs a value"))?;
                arch = Some(
                    Arch::parse(value)
                        .ok_or_else(|| anyhow::anyhow!("unknown architecture `{value}`"))?,
                );
            }
            "--release" => release = true,
            "--debug" => release = false,
            other => bail!("build brush: unknown argument `{other}`"),
        }
        index += 1;
    }
    Ok(BuildOptions {
        arch: arch.ok_or_else(|| anyhow::anyhow!("build brush: --arch is required"))?,
        release,
    })
}

/// Intercept `build brush` before the generic component parser sees it.
pub(crate) fn try_run(args: &[String]) -> Option<Result<u8>> {
    if args.first().map(String::as_str) != Some("build")
        || args.get(1).map(String::as_str) != Some("brush")
    {
        return None;
    }
    Some(run_build(&args[2..]))
}

fn run_build(args: &[String]) -> Result<u8> {
    let options = parse_build(args)?;
    let cwd = std::env::current_dir().context("cannot determine the current directory")?;
    let root = crate::repo::find_root(&cwd)?;
    build(&root, options)
}

fn build(root: &Path, options: BuildOptions) -> Result<u8> {
    let manifest = root.join("userland/brush/Cargo.toml");
    if !manifest.is_file() {
        bail!(
            "userland/brush is not vendored (missing {})",
            manifest.display()
        );
    }
    let crt_dir = root.join("build").join(options.arch.as_str());
    let crt1 = crt_dir.join("crt1.o");
    let crti = crt_dir.join("crti.o");
    let crtn = crt_dir.join("crtn.o");
    let libc = crt_dir.join("libc.a");
    for required in [&crt1, &crti, &crtn, &libc] {
        if !required.is_file() {
            bail!(
                "missing {}; run `cargo xtask libc-pack --arch {}` first",
                required.display(),
                options.arch.as_str()
            );
        }
    }

    let target = target_for(options.arch);
    let target_dir = root
        .join("build/cargo")
        .join(format!("brush-{}", options.arch.as_str()));
    // A std binary is PIE by default. The Raven spawn loader wants a fixed-address ET_EXEC with
    // page-aligned segments, so use the same static link script and code model as uutils.
    let linker = root
        .join("xtask")
        .join(format!("brush-{}.ld", options.arch.as_str()));
    if options.arch == Arch::X86_64 && !linker.is_file() {
        bail!("missing brush linker script: {}", linker.display());
    }
    let code_model = if options.arch == Arch::X86_64 {
        " -C code-model=small"
    } else {
        ""
    };
    let linker_arg = if options.arch == Arch::X86_64 {
        format!(" -C link-arg=-T{}", linker.display())
    } else {
        String::new()
    };
    let flags = format!(
        "--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static{}{} \
         -C link-arg={} -C link-arg={} -C link-arg=-L{} -C link-arg=-lc -C link-arg={}",
        code_model,
        linker_arg,
        crt1.display(),
        crti.display(),
        crt_dir.display(),
        crtn.display()
    );
    let mut cargo_args = vec![
        "+ravenbsd".to_string(),
        "build".to_string(),
        "--manifest-path".to_string(),
        manifest.display().to_string(),
        "-p".to_string(),
        PACKAGE.to_string(),
        "--bin".to_string(),
        BINARY.to_string(),
        "--target".to_string(),
        target.to_string(),
        "--no-default-features".to_string(),
        "--features".to_string(),
        "basic,reedline".to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
    ];
    if options.release {
        cargo_args.push("--release".to_string());
    }
    let code = run_command(
        &CommandSpec::new("cargo", cargo_args, root)
            .env("RUSTFLAGS", flags)
            .env_remove("CARGO_ENCODED_RUSTFLAGS"),
    )?;
    if code != 0 {
        return Ok(u8::from(code != 0));
    }

    let profile = if options.release { "release" } else { "debug" };
    let artifact = target_dir.join(target).join(profile).join(BINARY);
    if !artifact.is_file() {
        bail!(
            "brush built but the artifact is missing: {}",
            artifact.display()
        );
    }
    let arch_staged = root.join("build").join(options.arch.as_str()).join(BINARY);
    let staged = root.join(STAGED);
    copy(&artifact, &arch_staged)?;
    copy(&artifact, &staged)?;
    println!("{}", staged.display());
    Ok(0)
}

fn copy(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::copy(source, destination).with_context(|| {
        format!(
            "cannot copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

/// Map the short qtest name to the payload under `tests/brush/`.
///
/// The qtest resolver only scans `tests/qtest/cases`; the brush payload lives
/// in the ticket's Owns path, so an explicit path is passed through.
pub(crate) fn rewrite_qtest(args: Vec<String>) -> Vec<String> {
    let mut args = args;
    if args.first().map(String::as_str) != Some("qtest") {
        return args;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--case")
        && args.get(index + 1).map(String::as_str) == Some("shell-brush")
    {
        args[index + 1] = PathBuf::from("tests/brush/shell-brush.toml")
            .display()
            .to_string();
    }
    args
}

#[cfg(test)]
#[path = "brush_ut.rs"]
mod tests;
