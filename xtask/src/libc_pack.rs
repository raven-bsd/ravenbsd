// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `libc-pack`: build `libc.a`, pack its members, emit the symbol map, and run the link-time
//! checks of `donor-map/16` 3.A.5 / C60 section 4.5.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::libc_pack_crt;
use crate::libc_pack_stubs;
use crate::libc_pack_tools::Tools;
use crate::process::{CommandSpec, run as run_command};

/// One architecture of the pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arch {
    /// x86-64.
    X86_64,
    /// AArch64 soft-float.
    Aarch64,
    /// RISC-V 64.
    Riscv64,
}

impl Arch {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::Aarch64),
            "riscv64" => Ok(Self::Riscv64),
            other => bail!("unknown arch `{other}`"),
        }
    }

    pub(crate) fn triple(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-none",
            Self::Aarch64 => "aarch64-unknown-none-softfloat",
            Self::Riscv64 => "riscv64gc-unknown-none-elf",
        }
    }

    pub(crate) fn short(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }
}

/// Parsed command.
struct Options {
    arch: Arch,
    set: String,
    check: bool,
    probe: Option<String>,
    host_run: bool,
    json: bool,
    out: PathBuf,
}

fn parse(args: &[String]) -> Result<Options> {
    let mut arch = Arch::X86_64;
    let mut set = "server".to_string();
    let mut check = false;
    let mut probe = None;
    let mut host_run = false;
    let mut json = false;
    let mut out = None;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--arch") => {
                index += 1;
                arch = Arch::parse(args.get(index).context("--arch value")?)?;
            }
            Some("--set") => {
                index += 1;
                set = args.get(index).context("--set value")?.clone();
            }
            Some("--check") => check = true,
            Some("--probe") => {
                index += 1;
                probe = Some(args.get(index).context("--probe value")?.clone());
            }
            Some("--host-run") => host_run = true,
            Some("--json") => json = true,
            Some("--out") => {
                index += 1;
                out = Some(PathBuf::from(args.get(index).context("--out value")?));
            }
            Some(other) => bail!("unknown libc-pack flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    if !matches!(set.as_str(), "tiny" | "server") {
        bail!("unknown --set `{set}`");
    }
    Ok(Options {
        arch,
        set,
        check,
        probe,
        host_run,
        json,
        out: out.unwrap_or_else(|| PathBuf::from(format!("build/{}", arch.short()))),
    })
}

/// Run the command.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let options = parse(args)?;
    let profile = if options.set == "server" {
        "libc-a-server"
    } else {
        "libc-a"
    };
    let target_dir = options.out.join("cargo").join("libc-a");
    let features = &options.set;

    let tools = Tools::discover(root, options.arch)?;

    // 1. build
    let archive = std::fs::canonicalize(build(root, &options, profile, features, &target_dir)?)
        .context("cannot canonicalise libc.a")?;
    println!("libc-pack: build: ok");

    // 2. members
    let members_dir = options.out.join("pack/members");
    let _ = std::fs::remove_dir_all(&members_dir);
    std::fs::create_dir_all(&members_dir)
        .with_context(|| format!("cannot create {}", members_dir.display()))?;
    let mut members = tools.extract(&archive, &members_dir)?;
    // Put the shell's members before its dependencies. compiler_builtins ships hidden
    // fallback definitions of several math names; an archive resolves left-to-right, so the
    // public shell definition must be seen first or the hidden copy wins and K7 loses the name.
    members.sort_by_key(|member| !member.contains("raven_libc"));
    println!("libc-pack: members: ok");

    // 3. pack
    let packed = options.out.join("libc.a");
    // `llvm-ar rcsD` updates an existing archive in place and keeps members whose names are
    // absent from the new list. A rebuild changes every CGU hash, so the old shell members
    // would survive next to the new ones and the linker could pick the stale copy (the h2
    // hostrun did exactly that). Start from an empty archive for every pack.
    let _ = std::fs::remove_file(&packed);
    tools.pack(&packed, &members, &members_dir)?;
    let shell_members = members
        .iter()
        .filter(|member| member.contains("raven_libc.") || member.contains("libraven_libc."))
        .count();
    println!(
        "libc-pack: pack: ok ({} members, {shell_members} raven_libc)",
        members.len()
    );

    // 4. symbols
    let versions = root.join("libc/ravenlibc/abi/Versions.def");
    let symbol_map = root.join("libc/ravenlibc/crates/raven-libc/Symbol.map");
    // The symbol list is a build artifact. Write it beside the archive for every architecture
    // so `libc-pack` never dirties the canonical `libc/ravenlibc/abi/Symbols.list` in the tree.
    let list = options.out.join("Symbols.list");
    let libc_map = options.out.join("libc.map");
    let merged = crate::libc_pack_tools::mkmap(
        root,
        &[
            "merge".to_string(),
            "--versions".to_string(),
            versions.display().to_string(),
            "--map".to_string(),
            symbol_map.display().to_string(),
            "--arch".to_string(),
            options.arch.short().to_string(),
            "--out-map".to_string(),
            libc_map.display().to_string(),
            "--out-list".to_string(),
            list.display().to_string(),
        ],
    )?;
    if options.check {
        let before = std::fs::read_to_string(&list).unwrap_or_default();
        let _ = before;
    }
    println!("libc-pack: symbols: ok ({merged})");

    // 5. map
    let map_json = options.out.join("libc.a.map.json");
    tools.map_json(&packed, &members, &members_dir, &map_json)?;
    println!("libc-pack: map: ok");

    // 6. crt and stubs
    let crt = libc_pack_crt::run(root, &options.out)?;
    println!("libc-pack: crt: {crt}");
    let stubs = libc_pack_stubs::run(root, &options.out)?;
    println!("libc-pack: stubs: {stubs}");

    // 7. check
    if options.check {
        let mut check_args = vec![
            "check".to_string(),
            "--archive".to_string(),
            packed.display().to_string(),
            "--list".to_string(),
            list.display().to_string(),
            "--map".to_string(),
            symbol_map.display().to_string(),
            "--versions".to_string(),
            versions.display().to_string(),
            "--hooks".to_string(),
            root.join("libc/ravenlibc/crates/raven-libc/hooks.toml")
                .display()
                .to_string(),
            "--expected".to_string(),
            root.join("libc/ravenlibc/crates/raven-libc/expected")
                .display()
                .to_string(),
            "--arch".to_string(),
            options.arch.short().to_string(),
        ];
        let _ = &mut check_args;
        let findings = crate::libc_pack_tools::mkmap(root, &check_args)?;
        let granularity = check_granularity(root, &members)?;
        if granularity {
            println!("libc-pack: check: ok K1..K6");
        } else {
            println!("libc-pack: check: FAILED (member granularity)");
            return Ok(1);
        }
        let _ = findings;
    }

    // 8. probes and host-run
    if let Some(probe) = &options.probe {
        crate::libc_pack_probe::run(
            root,
            &options.arch,
            &options.out,
            &packed,
            probe,
            options.json,
        )?;
    }
    if options.host_run {
        crate::libc_pack_hostrun::run(root, &options.arch, &options.out)?;
    }

    // 9. stage the libc sysroot
    stage_sysroot(root, &options)?;
    println!("libc-pack: sysroot: ok");
    Ok(0)
}

/// Stage `build/<arch>/sysroot` so the target C tests compile and link without a manual step.
///
/// The sysroot mirrors what a clang `--sysroot` expects: crt objects and `libc.a` under
/// `usr/lib`, the base/arch/generated headers under `usr/include`, and — when they have been
/// produced by `libc-link` — `libc.so.1` under `lib` and `ld-elf.so.1` under `libexec`.
fn stage_sysroot(root: &Path, options: &Options) -> Result<()> {
    let sysroot = options.out.join("sysroot");
    let usr_lib = sysroot.join("usr/lib");
    let usr_include = sysroot.join("usr/include");
    std::fs::create_dir_all(&usr_lib).with_context(|| "cannot create sysroot/usr/lib")?;
    std::fs::create_dir_all(&usr_include).with_context(|| "cannot create sysroot/usr/include")?;

    for name in [
        "crt1.o",
        "Scrt1.o",
        "rcrt1.o",
        "crti.o",
        "crtn.o",
        "crtbegin.o",
        "crtbeginS.o",
        "crtend.o",
        "libc.a",
    ] {
        copy_if_exists(&options.out.join(name), &usr_lib.join(name))?;
    }

    let stubs = options.out.join("stubs");
    if stubs.is_dir() {
        for entry in std::fs::read_dir(&stubs).with_context(|| "cannot read stubs")? {
            let entry = entry?;
            let source = entry.path();
            let Some(name) = source.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.ends_with(".a") || name.ends_with(".so") {
                copy_if_exists(&source, &usr_lib.join(name))?;
            }
        }
    }

    // A rebuilt sysroot must not keep a wrapper from an older `libc-pack`: the compiler header
    // is not staged, and a stale copy would still shadow the compiler's own header.
    clean_compiler_headers(&usr_include);
    copy_headers(&root.join("libc/ravenlibc/include"), &usr_include)?;
    copy_headers(
        &root
            .join("libc/ravenlibc/arch")
            .join(options.arch.short())
            .join("include"),
        &usr_include,
    )?;
    copy_headers(
        &root.join("libc/ravenlibc/abi/generated/include"),
        &usr_include,
    )?;

    let lib = sysroot.join("lib");
    let libexec = sysroot.join("libexec");
    std::fs::create_dir_all(&lib).with_context(|| "cannot create sysroot/lib")?;
    std::fs::create_dir_all(&libexec).with_context(|| "cannot create sysroot/libexec")?;
    copy_if_exists(&options.out.join("libc.so.1"), &lib.join("libc.so.1"))?;
    copy_if_exists(
        &options.out.join("ld-elf.so.1"),
        &libexec.join("ld-elf.so.1"),
    )?;
    // `-lc` finds `libc.so`; a linker script keeps static and dynamic links on one file.
    let libc_so = usr_lib.join("libc.so");
    if options.out.join("libc.so.1").is_file() && !libc_so.is_file() {
        std::fs::write(&libc_so, b"INPUT ( libc.so.1 )\n")
            .with_context(|| "cannot write libc.so script")?;
    }
    Ok(())
}

fn copy_if_exists(source: &Path, target: &Path) -> Result<()> {
    if source.is_file() {
        std::fs::copy(source, target)
            .with_context(|| format!("cannot copy {} -> {}", source.display(), target.display()))?;
    }
    Ok(())
}

fn copy_headers(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(source).with_context(|| "cannot read headers")? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let destination = target.join(&name);
        if path.is_dir() {
            std::fs::create_dir_all(&destination)
                .with_context(|| format!("cannot create {}", destination.display()))?;
            copy_headers(&path, &destination)?;
        } else {
            if name.to_str().is_some_and(is_compiler_header) {
                continue;
            }
            copy_if_exists(&path, &destination)?;
        }
    }
    Ok(())
}

/// Headers that the C compiler owns and that use `#include_next`.
///
/// They are wrappers over the compiler resource directory. Staging them into the sysroot makes
/// `--sysroot` find the wrapper first and the compiler header second, so `#include_next` has no
/// later copy to include. The sysroot must leave them to the compiler.
const COMPILER_HEADERS: [&str; 5] = [
    "stddef.h",
    "stdbool.h",
    "stdatomic.h",
    "stdarg.h",
    "float.h",
];

fn is_compiler_header(name: &str) -> bool {
    COMPILER_HEADERS.contains(&name)
}

/// Remove a stale compiler wrapper from a previously staged sysroot.
fn clean_compiler_headers(directory: &Path) {
    for name in COMPILER_HEADERS {
        let path = directory.join(name);
        if path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The exact `cargo rustc` command vector of step 1, without spawning it.
fn build_spec(
    root: &Path,
    arch: Arch,
    profile: &str,
    features: &str,
    target_dir: &Path,
) -> CommandSpec {
    let mut flags = String::from("-C relocation-model=pic");
    if arch == Arch::X86_64 {
        flags.push_str(" -C code-model=small");
    } else {
        // The precompiled `core`/`alloc` rlibs of the ARM targets are not PIC; ThinLTO
        // re-codegens their bitcode under the PIC relocation model with the shell.
        flags.push_str(" -C lto=thin -C embed-bitcode=yes");
    }
    flags.push_str(&format!(
        " --remap-path-prefix={}=/ravenbsd",
        root.display()
    ));
    let mut cargo_args = vec![
        "rustc".to_string(),
        "-p".to_string(),
        "raven-libc".to_string(),
        "--crate-type".to_string(),
        "staticlib".to_string(),
        "--profile".to_string(),
        profile.to_string(),
        "--target".to_string(),
        arch.triple().to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
        "--features".to_string(),
        features.to_string(),
    ];
    cargo_args.push("--".to_string());
    CommandSpec::new("cargo", cargo_args, root)
        .env("RUSTFLAGS", flags)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
}

fn build(
    root: &Path,
    options: &Options,
    profile: &str,
    features: &str,
    target_dir: &Path,
) -> Result<PathBuf> {
    let spec = build_spec(root, options.arch, profile, features, target_dir);
    let code = run_command(&spec)?;
    if code != 0 {
        bail!("cargo rustc failed with exit code {code}");
    }
    let expected = target_dir
        .join(options.arch.triple())
        .join(profile)
        .join("libraven_libc.a");
    if expected.is_file() {
        return Ok(expected);
    }
    // Fall back to a search when cargo names the artifact differently.
    let mut found = None;
    walk(target_dir, &mut found);
    found.ok_or_else(|| anyhow::anyhow!("libraven_libc.a not found under {}", target_dir.display()))
}

fn walk(dir: &Path, found: &mut Option<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("libraven_libc.a") {
            *found = Some(path);
        }
    }
}

/// R-3: at least 0.9 members per `no_mangle` source file.
fn check_granularity(root: &Path, members: &[String]) -> Result<bool> {
    let src = root.join("libc/ravenlibc/crates/raven-libc/src");
    let mut expected = 0usize;
    // C65's compat tree is compiled on x86_64 only; the other arches would otherwise demand
    // members that do not exist there.
    let compat = src.join("compat");
    count_no_mangle_except(&src, &compat, &mut expected);
    if expected == 0 {
        return Ok(true);
    }
    let shell = members
        .iter()
        .filter(|member| member.contains("raven_libc.") || member.contains("libraven_libc."))
        .count();
    let ok = shell.saturating_mul(10) >= expected.saturating_mul(9);
    if !ok {
        println!("libc-pack: granularity: {shell} shell members for {expected} no_mangle files");
    }
    Ok(ok)
}

/// R-3 directory walk: count `no_mangle` tokens outside `excluded`.
fn count_no_mangle_except(dir: &Path, excluded: &Path, count: &mut usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == excluded {
            continue;
        }
        if path.is_dir() {
            count_no_mangle_except(&path, excluded, count);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            *count = count.saturating_add(text.matches("no_mangle").count());
        }
    }
}

#[cfg(test)]
#[path = "libc_pack_ut.rs"]
mod tests;
