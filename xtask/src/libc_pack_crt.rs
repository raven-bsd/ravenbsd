// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `crt` step of `libc-pack` (C61 4.4): build `raven-libc-crt` as a staticlib, extract the
//! archive member that defines each start marker, fall back to one `rustc --emit=obj` run when
//! the primary path is ambiguous, strip the extractor-only symbols and verify every object
//! against the checks of C61 4.7.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::libc_pack::Arch;
use crate::libc_pack_tools::Tools;
use crate::process::{CommandSpec, run as run_command};

/// The eight start objects, in output order.
const OBJECTS: [(&str, &str); 8] = [
    ("crt1", "crt1"),
    ("Scrt1", "scrt1"),
    ("rcrt1", "rcrt1"),
    ("crti", "crti"),
    ("crtn", "crtn"),
    ("crtbegin", "crtbegin"),
    ("crtbeginS", "crtbegin_s"),
    ("crtend", "crtend"),
];

/// Run the step. `out` is `build/<arch>`; the arch is its last path component.
pub(crate) fn run(root: &Path, out: &Path) -> Result<String> {
    if !root.join("libc/ravenlibc/crt/Cargo.toml").is_file() {
        return Ok("skipped(no libc/ravenlibc/crt)".to_string());
    }
    let Some(arch_name) = out.file_name().and_then(|name| name.to_str()) else {
        return Ok("skipped(no arch in --out)".to_string());
    };
    let arch = match arch_name {
        "x86_64" => Arch::X86_64,
        "aarch64" => Arch::Aarch64,
        "riscv64" => Arch::Riscv64,
        other => return Ok(format!("skipped(unknown arch `{other}`)")),
    };
    let crt = root.join("libc/ravenlibc/crt");
    let build = out.join("cargo").join("libc-crt");
    let archive = std::path::absolute(build_staticlib(root, arch, &build)?)?;

    let members_dir = out.join("pack").join("crt-members");
    let _ = std::fs::remove_dir_all(&members_dir);
    std::fs::create_dir_all(&members_dir)
        .with_context(|| format!("cannot create {}", members_dir.display()))?;
    let tools = Tools::discover(root, arch)?;
    let members = tools.extract(&archive, &members_dir)?;
    let crate_members = members
        .iter()
        .filter(|member| member.contains(".raven_libc_crt."))
        .cloned()
        .collect::<Vec<_>>();
    if crate_members.is_empty() {
        bail!("no raven_libc_crt member in {}", archive.display());
    }

    let objcopy = find_tool("LLVM_OBJCOPY", &["llvm-objcopy", "objcopy"])?;
    let readelf = find_tool("LLVM_READELF", &["llvm-readelf", "readelf"])?;

    for (object, source) in OBJECTS {
        let marker = format!("__raven_crt_kind_{source}");
        let owners = members_with_marker(&tools.nm, &members_dir, &crate_members, &marker)?;
        let output = out.join(format!("{object}.o"));
        match owners.as_slice() {
            [member] => {
                let source_path = members_dir.join(member);
                std::fs::copy(&source_path, &output).with_context(|| {
                    format!(
                        "cannot copy {} to {}",
                        source_path.display(),
                        output.display()
                    )
                })?;
                strip_object(&objcopy, &output, &marker, &tools.nm)?;
            }
            _ => {
                println!("crt: fallback({object})");
                build_fallback(&crt, arch, source, &output)?;
                strip_object(&objcopy, &output, &marker, &tools.nm)?;
            }
        }
        if matches!(object, "crti" | "crtn") {
            remove_section(&objcopy, &output, ".text")?;
        }
        verify(root, &readelf, &tools.nm, object, &output)?;
    }
    Ok("ok (8 objects)".to_string())
}

/// Build the crate as a staticlib with the `libc-a` profile flags of C60 4.2.
fn build_staticlib(root: &Path, arch: Arch, build: &Path) -> Result<PathBuf> {
    let mut flags = String::from("-C relocation-model=pic");
    if arch == Arch::X86_64 {
        flags.push_str(" -C code-model=small");
    }
    flags.push_str(&format!(
        " --remap-path-prefix={}=/ravenbsd",
        root.display()
    ));
    let mut args = vec![
        "rustc".to_string(),
        "-p".to_string(),
        "raven-libc-crt".to_string(),
        "--crate-type".to_string(),
        "staticlib".to_string(),
        "--profile".to_string(),
        "libc-a".to_string(),
        "--target".to_string(),
        arch.triple().to_string(),
        "--target-dir".to_string(),
        build.display().to_string(),
    ];
    args.push("--".to_string());
    let spec = CommandSpec::new("cargo", args, root)
        .env("RUSTFLAGS", flags)
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    let code = run_command(&spec)?;
    if code != 0 {
        bail!("cargo rustc -p raven-libc-crt failed with exit code {code}");
    }
    let expected = build
        .join(arch.triple())
        .join("libc-a")
        .join("libraven_libc_crt.a");
    if expected.is_file() {
        return Ok(expected);
    }
    let mut found = None;
    walk(build, &mut found);
    found.ok_or_else(|| anyhow::anyhow!("libraven_libc_crt.a not found under {}", build.display()))
}

fn walk(dir: &Path, found: &mut Option<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else if path.file_name().and_then(|name| name.to_str()) == Some("libraven_libc_crt.a") {
            *found = Some(path);
        }
    }
}

/// Members that define `marker`.
fn members_with_marker(
    nm: &Path,
    dir: &Path,
    candidates: &[String],
    marker: &str,
) -> Result<Vec<String>> {
    let mut owners = Vec::new();
    for member in candidates {
        let output = Command::new(nm)
            .args(["--defined-only", "--format=posix"])
            .arg(member)
            .current_dir(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .with_context(|| format!("cannot run nm on {member}"))?;
        let text = String::from_utf8_lossy(&output.stdout);
        if text.lines().any(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|name| name == marker)
        }) {
            owners.push(member.clone());
        }
    }
    Ok(owners)
}

/// The E2 fallback: one `rustc --emit=obj` run per object.
fn build_fallback(crt: &Path, arch: Arch, source: &str, output: &Path) -> Result<()> {
    let root = crt.join("objs").join(format!("{source}.rs"));
    let status = Command::new("rustc")
        .args(["--edition", "2024", "--crate-name", source])
        .args(["--crate-type", "lib", "--emit=obj"])
        .args(["--target", arch.triple()])
        .args([
            "-C",
            "panic=abort",
            "-C",
            "opt-level=z",
            "-C",
            "relocation-model=pic",
        ])
        .arg("-o")
        .arg(output)
        .arg(&root)
        .stdout(Stdio::null())
        .status()
        .with_context(|| format!("cannot run rustc on {}", root.display()))?;
    if !status.success() {
        bail!("the fallback build of {} failed", root.display());
    }
    Ok(())
}

/// Remove `.comment`, the extractor marker and the crate `FILE` symbol.
///
/// The `FILE` symbol (`<crate>.<hash>-cgu.N`) is what makes the E1 member and the E2 object
/// differ byte for byte; stripping it makes both paths reproducible and comparable (C61 4.7).
fn strip_object(objcopy: &Path, object: &Path, marker: &str, nm: &Path) -> Result<()> {
    let cleanup = object.with_extension("clean.o");
    let status = Command::new(objcopy)
        .args(["--remove-section", ".comment"])
        .arg(format!("--strip-symbol={marker}"))
        .arg(object)
        .arg(&cleanup)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("cannot run {}", objcopy.display()))?;
    if !status.success() {
        bail!("{} failed on {}", objcopy.display(), object.display());
    }
    for file in file_symbols(nm, &cleanup)? {
        let next = object.with_extension("clean2.o");
        let status = Command::new(objcopy)
            .arg(format!("--strip-symbol={file}"))
            .arg(&cleanup)
            .arg(&next)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("cannot strip the FILE symbol of {}", object.display()))?;
        if !status.success() {
            bail!("{} failed on {}", objcopy.display(), cleanup.display());
        }
        let _ = std::fs::rename(&next, &cleanup);
    }
    std::fs::rename(&cleanup, object)
        .with_context(|| format!("cannot replace {}", object.display()))?;
    Ok(())
}

/// Remove a section from a produced object (the empty `.text` of `crti`/`crtn`).
fn remove_section(objcopy: &Path, object: &Path, section: &str) -> Result<()> {
    let next = object.with_extension("nosection.o");
    let status = Command::new(objcopy)
        .arg(format!("--remove-section={section}"))
        .arg(object)
        .arg(&next)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("cannot remove {section} from {}", object.display()))?;
    if !status.success() {
        bail!("{} failed to remove {section}", objcopy.display());
    }
    std::fs::rename(&next, object)
        .with_context(|| format!("cannot replace {}", object.display()))?;
    Ok(())
}

/// The local absolute `FILE` symbols of an object (`<...>-cgu.N`).
fn file_symbols(nm: &Path, object: &Path) -> Result<Vec<String>> {
    let output = Command::new(nm)
        .args(["-a", "--format=posix"])
        .arg(object)
        .output()
        .with_context(|| format!("cannot run nm on {}", object.display()))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut names = Vec::new();
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 2 && fields.get(1) == Some(&"a") && fields[0].contains("-cgu.") {
            names.push(fields[0].to_string());
        }
    }
    Ok(names)
}

/// The checks of C61 4.7 for one produced object.
fn verify(root: &Path, readelf: &Path, nm: &Path, object: &str, path: &Path) -> Result<()> {
    let sections = capture(readelf, &["-S", "-W"], path)?;
    if !sections.contains(".note.GNU-stack") {
        bail!("{object}.o has no .note.GNU-stack");
    }
    for line in sections.lines() {
        if line.contains(".note.GNU-stack") && line.split_whitespace().any(|word| word == "X") {
            bail!("{object}.o has an executable .note.GNU-stack");
        }
        for forbidden in [".init", ".fini", ".ctors", ".init_array", ".preinit_array"] {
            if line
                .split_whitespace()
                .any(|word| word == forbidden || word.starts_with(&format!("{forbidden}.")))
            {
                bail!("{object}.o contains the forbidden section {forbidden}");
            }
        }
    }

    let symbols = capture(nm, &["--defined-only", "--format=posix"], path)?;
    let defined = symbols
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let name = fields.first()?;
            let kind = fields.get(1)?;
            Some(((*name).to_string(), (*kind).to_string()))
        })
        .collect::<Vec<_>>();
    if defined
        .iter()
        .any(|(name, _)| name.starts_with("__raven_crt_kind_"))
    {
        bail!("{object}.o still defines its extractor marker");
    }
    match object {
        "crt1" | "Scrt1" | "rcrt1" => {
            let mut text = defined
                .iter()
                .filter(|(_, kind)| kind == "T")
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>();
            // `rcrt1.o` also carries the stage-1 Rust function that runs the self-relocator
            // (C61 4.3); the other two entry objects define `_start` only.
            if object == "rcrt1" {
                text.retain(|name| *name != "__raven_rcrt1_stage1");
            }
            if text.as_slice() != ["_start"] {
                bail!("{object}.o defines {text:?} instead of exactly `_start`");
            }
        }
        "crti" | "crtn" => {
            if !defined.is_empty() {
                bail!("{object}.o defines {} global symbols", defined.len());
            }
        }
        "crtbegin" => {
            if !defined.iter().any(|(name, _)| name == "__dso_handle") {
                bail!("crtbegin.o does not define __dso_handle");
            }
        }
        "crtbeginS" => {
            for name in ["__dso_handle", "__do_fini"] {
                if !defined.iter().any(|(symbol, _)| symbol == name) {
                    bail!("crtbeginS.o does not define {name}");
                }
            }
            if !sections.contains(".fini_array") {
                bail!("crtbeginS.o has no .fini_array section");
            }
        }
        "crtend" => {}
        _ => {}
    }
    if object == "crtend" && !sections.contains(".eh_frame") {
        bail!("crtend.o has no .eh_frame section");
    }

    if matches!(object, "crt1" | "Scrt1" | "rcrt1") {
        let notes = capture(readelf, &["-n"], path)?;
        if !notes.contains("RavenBSD") || !notes.contains("01 00 00 00") {
            bail!("{object}.o lacks the .note.raven.abi name/descriptor");
        }
    }

    if object == "rcrt1" {
        let relocations = capture(readelf, &["-r", "-W"], path)?;
        for line in relocations.lines() {
            let upper = line.to_ascii_uppercase();
            if upper.contains("GOT") || upper.contains("GLOB_DAT") || upper.contains("JUMP_SLOT") {
                bail!("rcrt1.o has a GOT-class relocation: {}", line.trim());
            }
        }
    }
    let _ = root;
    Ok(())
}

fn capture(program: &Path, args: &[&str], last: &Path) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .arg(last)
        .output()
        .with_context(|| format!("cannot run {}", program.display()))?;
    if !output.status.success() {
        bail!("{} failed on {}", program.display(), last.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Find an object tool: `$ENV`, `$LLVM_BIN_DIR`, the rustc sysroot, or `PATH`.
fn find_tool(env: &str, names: &[&str]) -> Result<PathBuf> {
    if let Ok(value) = std::env::var(env)
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }
    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string()));
    let host = host_triple();
    let sysroot_bin = sysroot.map(|root| root.join("lib/rustlib").join(&host).join("bin"));
    let llvm_dir = std::env::var_os("LLVM_BIN_DIR").map(PathBuf::from);
    for name in names {
        if let Some(dir) = &llvm_dir {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        if let Some(dir) = &sysroot_bin {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    bail!("{} not found", names.first().copied().unwrap_or("tool"))
}

fn host_triple() -> String {
    let output = Command::new("rustc")
        .args(["-vV"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    for line in output.lines() {
        if let Some(value) = line.strip_prefix("host: ") {
            return value.trim().to_string();
        }
    }
    "x86_64-unknown-linux-gnu".to_string()
}
