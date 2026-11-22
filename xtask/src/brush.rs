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
use raven_elf::{Elf, LoadMode, Machine, plan_load};

use crate::build::Arch;
use crate::process::{CommandSpec, run as run_command};

/// The package and binary built by this component.
const PACKAGE: &str = "brush-shell";
/// The binary name produced by the package.
const BINARY: &str = "brush";

/// The arch-correct staging path for the brush binary.
pub(crate) fn staged_for(arch: Arch) -> String {
    format!("build/{}/brush", arch.as_str())
}

/// Build release brush for one architecture; used by the image/initfs prerequisite path.
pub(crate) fn build_for_arch(root: &Path, arch: Arch) -> Result<u8> {
    build(
        root,
        BuildOptions {
            arch,
            release: true,
            check: false,
        },
    )
}

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
    check: bool,
}

fn parse_build(args: &[String]) -> Result<BuildOptions> {
    let mut arch = None;
    // The target shell is installed in the boot image; a debug brush carries megabytes of DWARF
    // and no longer fits the ESP. Release is the default, as for the uutils target build.
    let mut release = true;
    let mut check = false;
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
            "--check" => check = true,
            other => bail!("build brush: unknown argument `{other}`"),
        }
        index += 1;
    }
    Ok(BuildOptions {
        arch: arch.ok_or_else(|| anyhow::anyhow!("build brush: --arch is required"))?,
        release,
        check,
    })
}

/// The link script selected for one architecture.
///
/// RISC-V has no brush build today; it keeps the old lld-default behavior.
fn linker_for(arch: Arch) -> Option<PathBuf> {
    match arch {
        Arch::X86_64 | Arch::Aarch64 => {
            Some(PathBuf::from(format!("xtask/brush-{}.ld", arch.as_str())))
        }
        Arch::Riscv64 => None,
    }
}

/// Resolve the selected script and require it before cargo runs.
fn selected_linker(root: &Path, arch: Arch) -> Result<Option<PathBuf>> {
    let Some(relative) = linker_for(arch) else {
        return Ok(None);
    };
    let path = root.join(relative);
    if !path.is_file() {
        bail!("missing brush linker script: {}", path.display());
    }
    Ok(Some(path))
}

fn machine_for(arch: Arch) -> Machine {
    match arch {
        Arch::X86_64 => Machine::X86_64,
        Arch::Aarch64 => Machine::Aarch64,
        Arch::Riscv64 => Machine::Riscv64,
    }
}

/// Run the real `raven-elf` Strict planner on a staged artifact.
fn verify_strict(path: &Path, arch: Arch) -> Result<()> {
    // Mirrors raven-abi's user end; spawn parses at `0..USER_END`.
    const USER_END: u64 = 0x0000_7FFF_FFFF_F000;
    const PAGE: u64 = 4096;
    let bytes = std::fs::read(path)
        .with_context(|| format!("cannot read brush artifact {}", path.display()))?;
    let elf = Elf::parse(&bytes, machine_for(arch), 0..USER_END)
        .map_err(|error| anyhow::anyhow!("{}: ELF parse failed: {error:?}", path.display()))?;
    let plan = plan_load(&elf, PAGE, LoadMode::Strict)
        .map_err(|error| anyhow::anyhow!("{}: Strict plan = Err({error:?})", path.display()))?;
    println!("{}: Strict plan = Ok({})", path.display(), plan.count());
    Ok(())
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
    if options.check {
        let staged = root.join(staged_for(options.arch));
        if !staged.is_file() {
            bail!("brush artifact is missing: {}", staged.display());
        }
        verify_strict(&staged, options.arch)?;
        return Ok(0);
    }

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
    let linker = selected_linker(root, options.arch)?;
    let code_model = if options.arch == Arch::X86_64 {
        " -C code-model=small"
    } else {
        ""
    };
    let linker_arg = linker
        .as_ref()
        .map(|path| format!(" -C link-arg=-T{}", path.display()))
        .unwrap_or_default();
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
        "basic,reedline,minimal".to_string(),
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
    let staged = root.join(staged_for(options.arch));
    copy(&artifact, &staged)?;
    verify_strict(&staged, options.arch)?;
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
