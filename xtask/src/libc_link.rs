// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `libc-link`: build `libc.so.1` and `ld-elf.so.1`, then run the ELF/K7 gates.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::libc_pack::Arch;
use crate::process::{CommandSpec, run as run_command};

const LINKER_FLAVOR: &str = "-flavor";

/// The supported dynamic loader image base.
pub(crate) const LOADER_IMAGE_BASE: u64 = 0x1000;
/// The installed loader path every dynamic PIE requests as its interpreter.
pub(crate) const LOADER_INTERPRETER: &str = "/libexec/ld-elf.so.1";
/// Manifest written beside the linked loader and checked before reuse.
const LOADER_MANIFEST: &str = "loader-manifest.toml";

/// One parsed command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Options {
    /// The architecture.
    pub(crate) arch: Arch,
    /// `tiny` links nothing.
    pub(crate) set: String,
    /// Run the ELF gate.
    pub(crate) check: bool,
    /// Verify the DSO stamp instead of building.
    pub(crate) verify: bool,
    /// Print a JSON summary.
    pub(crate) json: bool,
    /// Output directory.
    pub(crate) out: PathBuf,
}

fn parse(args: &[String]) -> Result<Options> {
    let mut arch = Arch::X86_64;
    let mut set = "server".to_string();
    let mut check = false;
    let mut verify = false;
    let mut json = false;
    let mut out = None;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--arch") => {
                index += 1;
                arch = match args.get(index).map(String::as_str) {
                    Some("x86_64") => Arch::X86_64,
                    Some("aarch64") => Arch::Aarch64,
                    Some("riscv64") => Arch::Riscv64,
                    Some(other) => bail!("unknown arch `{other}`"),
                    None => bail!("--arch value"),
                };
            }
            Some("--set") => {
                index += 1;
                set = args.get(index).context("--set value")?.clone();
            }
            Some("--check") => check = true,
            Some("--verify") => verify = true,
            Some("--json") => json = true,
            Some("--out") => {
                index += 1;
                out = Some(PathBuf::from(args.get(index).context("--out value")?));
            }
            Some(other) => bail!("unknown libc-link flag `{other}`"),
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
        verify,
        json,
        out: out.unwrap_or_else(|| PathBuf::from(format!("build/{}", arch.short()))),
    })
}

/// Run `libc-link`.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let options = parse(args)?;
    if options.set == "tiny" {
        println!("libc-link: skipped(tiny)");
        return Ok(0);
    }
    if options.verify {
        if verify_dso_stamp(root, &options).is_ok() {
            println!("libc-link: verify: ok");
            return Ok(0);
        }
        eprintln!("artifact stale: run cargo xtask libc-link");
        return Ok(1);
    }
    let out = root.join(&options.out);
    std::fs::create_dir_all(&out).with_context(|| format!("cannot create {}", out.display()))?;
    let libc_archive = if options.arch == Arch::Aarch64 {
        // C74id: the DSO links the opt-level=z archive; the static `libc.a` stays opt-level=s so
        // the aarch64 static P9 rows are unchanged.
        out.join("dso/libc.a")
    } else {
        out.join("libc.a")
    };
    let libc_map = out.join("libc.map");
    let canonical = root.join("libc/ravenlibc/abi/Symbols.list");
    let per_arch = out.join("Symbols.list");
    let symbols = if options.arch == crate::libc_pack::Arch::X86_64 || !per_arch.is_file() {
        canonical
    } else {
        per_arch
    };
    if !libc_archive.is_file() || !libc_map.is_file() {
        println!("libc-link: mkmap: running libc-pack");
        let code = run_command(&CommandSpec::new(
            "cargo",
            vec![
                "xtask".into(),
                "libc-pack".into(),
                "--arch".into(),
                options.arch.short().into(),
                "--out".into(),
                out.display().to_string(),
            ],
            root,
        ))?;
        if code != 0 {
            bail!("libc-link: mkmap: FAILED (libc-pack exit {code})");
        }
    }
    let undef = out.join("libc.undef.rsp");
    write_undef(&symbols, &undef)?;
    println!("libc-link: mkmap: ok");

    let linker = find_linker(root)?;
    let libc = out.join("libc.so.1");
    let loader = out.join("ld-elf.so.1");
    let libc_ok = link_libc(
        root,
        &options,
        &linker,
        &libc_archive,
        &symbols,
        &libc_map,
        &undef,
        &libc,
    )?;
    if !libc_ok {
        println!("libc-link: libc.so.1: FAILED");
        return Ok(1);
    }
    println!("libc-link: libc.so.1: ok");
    let loader_ok = link_loader(root, &options, &linker, &loader)?;
    if !loader_ok {
        println!("libc-link: ld-elf.so.1: FAILED");
        return Ok(1);
    }
    println!("libc-link: ld-elf.so.1: ok");
    if let Err(error) = check_elf(&loader, "loader") {
        println!("libc-link: loader contract: FAILED: {error}");
        return Ok(1);
    }
    write_loader_manifest(root, options.arch.short(), &loader)?;
    println!(
        "libc-link: loader manifest: {}",
        loader_manifest_path(root, options.arch.short()).display()
    );
    stage_sysroot(&out, &libc, &loader)?;
    println!("libc-link: sysroot: ok");
    write_dso_stamp(root, &options, &libc, &loader)?;
    println!("libc-link: stamp: ok");

    if options.check {
        let mut failed = false;
        for (path, kind) in [(&libc, "libc"), (&loader, "loader")] {
            match check_elf(path, kind) {
                Ok(lines) => {
                    for line in lines {
                        println!("libc-link: check: {line}");
                    }
                }
                Err(error) => {
                    println!("libc-link: check: FAILED: {error}");
                    failed = true;
                }
            }
        }
        match check_mkmap(root, &options, &libc_archive, &symbols, &libc_map, &libc) {
            Ok(summary) => println!("libc-link: check: {summary}"),
            Err(error) => {
                println!("libc-link: check: FAILED: {error}");
                failed = true;
            }
        }
        if failed {
            return Ok(1);
        }
        println!("libc-link: check: ok");
    }
    if options.json {
        let libc_size = std::fs::metadata(&libc)
            .map(|value| value.len())
            .unwrap_or(0);
        let loader_size = std::fs::metadata(&loader)
            .map(|value| value.len())
            .unwrap_or(0);
        println!(
            "{{\"arch\":\"{}\",\"libc\":\"{}\",\"libc_size\":{},\"loader\":\"{}\",\"loader_size\":{}}}",
            options.arch.short(),
            libc.display(),
            libc_size,
            loader.display(),
            loader_size
        );
    }
    Ok(0)
}

/// DSO stamp file written after a successful `libc-link` (K441).
pub(crate) const DSO_STAMP_FILE: &str = "libc-dso.stamp";
const DSO_STAMP_FORMAT: &str = "k441.1";

fn write_dso_stamp(root: &Path, options: &Options, libc: &Path, loader: &Path) -> Result<()> {
    let out = root.join(&options.out);
    let text = format!(
        "format={DSO_STAMP_FORMAT}\narch={}\nset={}\nsources={:016x}\nlibc_so={:016x}\nsysroot_libc_so={:016x}\nloader={:016x}\nsysroot_loader={:016x}\n",
        options.arch.short(),
        options.set,
        crate::libc_pack::source_hash(root)?,
        crate::libc_pack::hash_file(libc)?,
        crate::libc_pack::hash_file(&out.join("sysroot/lib/libc.so.1"))?,
        crate::libc_pack::hash_file(loader)?,
        crate::libc_pack::hash_file(&out.join("sysroot/libexec/ld-elf.so.1"))?,
    );
    let path = out.join(DSO_STAMP_FILE);
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, text)
        .with_context(|| format!("cannot write {}", temporary.display()))?;
    std::fs::rename(&temporary, &path)
        .with_context(|| format!("cannot publish {}", path.display()))?;
    Ok(())
}

fn verify_dso_stamp(root: &Path, options: &Options) -> Result<()> {
    let out = root.join(&options.out);
    let text =
        std::fs::read_to_string(out.join(DSO_STAMP_FILE)).with_context(|| "no libc-dso stamp")?;
    if crate::libc_pack::stamp_field(&text, "format") != Some(DSO_STAMP_FORMAT) {
        bail!("stamp format mismatch");
    }
    if crate::libc_pack::stamp_field(&text, "arch") != Some(options.arch.short()) {
        bail!("stamp arch mismatch");
    }
    if crate::libc_pack::stamp_field(&text, "set") != Some(options.set.as_str()) {
        bail!("stamp set mismatch");
    }
    let field = |key: &str| -> Result<u64> {
        let value = crate::libc_pack::stamp_field(&text, key)
            .with_context(|| format!("stamp {key} missing"))?;
        u64::from_str_radix(value, 16).with_context(|| format!("stamp {key} invalid"))
    };
    if field("sources")? != crate::libc_pack::source_hash(root)? {
        bail!("sources changed");
    }
    for (key, path) in [
        ("libc_so", out.join("libc.so.1")),
        ("sysroot_libc_so", out.join("sysroot/lib/libc.so.1")),
        ("loader", out.join("ld-elf.so.1")),
        ("sysroot_loader", out.join("sysroot/libexec/ld-elf.so.1")),
    ] {
        if field(key)? != crate::libc_pack::hash_file(&path)? {
            bail!("{key} changed");
        }
    }
    Ok(())
}

/// Publish the freshly linked dynamic objects into the arch sysroot.
///
/// `libc-pack` stages the static sysroot; the dynamic `libc.so.1`/`ld-elf.so.1` are only known
/// after this step, so write them here so the C target cases link against the current build.
fn stage_sysroot(out: &Path, libc: &Path, loader: &Path) -> Result<()> {
    let sysroot = out.join("sysroot");
    let lib = sysroot.join("lib");
    let libexec = sysroot.join("libexec");
    let usr_lib = sysroot.join("usr/lib");
    for directory in [&lib, &libexec, &usr_lib] {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
    }
    std::fs::copy(libc, lib.join("libc.so.1"))
        .with_context(|| format!("cannot copy {} to sysroot/lib", libc.display()))?;
    std::fs::copy(loader, libexec.join("ld-elf.so.1"))
        .with_context(|| format!("cannot copy {} to sysroot/libexec", loader.display()))?;
    let libc_so = usr_lib.join("libc.so");
    // Always replace a stale sysroot `libc.so`. Older trees carry a real shared object here;
    // leaving it in place makes `-lc` prefer that stale image over the freshly linked
    // `lib/libc.so.1`, which is how the dynamic pty exports disappeared from C links.
    std::fs::write(&libc_so, b"INPUT ( libc.so.1 )\n")
        .with_context(|| "cannot write sysroot libc.so script")?;
    Ok(())
}

/// Loader contract record written after a successful `libc-link`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoaderManifest {
    /// Architecture short name.
    pub(crate) arch: String,
    /// Product source revision that produced the loader.
    pub(crate) revision: String,
    /// First `PT_LOAD` virtual address.
    pub(crate) image_base: String,
    /// Interpreter path the loader is installed as.
    pub(crate) interpreter: String,
    /// Loader file size in bytes.
    pub(crate) size: u64,
    /// CRC-32 of the loader bytes, the fast reuse check.
    pub(crate) crc32: String,
    /// SHA-256 of the loader bytes for the release inventory; `unavailable` when no tool exists.
    pub(crate) sha256: String,
}

/// Path of the loader manifest under `build/<arch>`.
pub(crate) fn loader_manifest_path(root: &Path, arch: &str) -> PathBuf {
    root.join("build").join(arch).join(LOADER_MANIFEST)
}

/// Render the manifest. Pure so host tests cover every field without a toolchain.
fn loader_manifest_text(arch: &str, revision: &str, bytes: &[u8], sha256: &str) -> String {
    format!(
        "# RavenBSD dynamic loader manifest\narch = \"{arch}\"\nrevision = \"{revision}\"\nimage_base = \"0x{:x}\"\ninterpreter = \"{}\"\nsize = {}\ncrc32 = \"{:08x}\"\nsha256 = \"{}\"\n",
        LOADER_IMAGE_BASE,
        LOADER_INTERPRETER,
        bytes.len(),
        raven_initfs::crc32(bytes),
        sha256,
    )
}

fn manifest_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches('"'))
    })
}

/// Parse the manifest; `None` when a required field is absent or unparseable.
fn parse_loader_manifest(text: &str) -> Option<LoaderManifest> {
    Some(LoaderManifest {
        arch: manifest_value(text, "arch")?.to_string(),
        revision: manifest_value(text, "revision")?.to_string(),
        image_base: manifest_value(text, "image_base")?.to_string(),
        interpreter: manifest_value(text, "interpreter")?.to_string(),
        size: manifest_value(text, "size")?.parse().ok()?,
        crc32: manifest_value(text, "crc32")?.to_string(),
        sha256: manifest_value(text, "sha256")?.to_string(),
    })
}

/// Read the loader manifest for `arch`.
pub(crate) fn read_loader_manifest(root: &Path, arch: &str) -> Option<LoaderManifest> {
    let text = std::fs::read_to_string(loader_manifest_path(root, arch)).ok()?;
    parse_loader_manifest(&text)
}

fn sha256_file(path: &Path) -> String {
    match Command::new("sha256sum").arg(path).output() {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("unavailable")
            .to_string(),
        _ => "unavailable".to_string(),
    }
}

fn source_revision(root: &Path) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_string())
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Write the loader manifest next to the freshly linked loader.
pub(crate) fn write_loader_manifest(root: &Path, arch: &str, loader: &Path) -> Result<()> {
    let bytes =
        std::fs::read(loader).with_context(|| format!("cannot read {}", loader.display()))?;
    let text = loader_manifest_text(arch, &source_revision(root), &bytes, &sha256_file(loader));
    let path = loader_manifest_path(root, arch);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// True when the manifest describes the loader currently on disk.
///
/// This is the reuse gate: a missing manifest or a different loader size/CRC forces a relink,
/// so an artifact from another checkout cannot be packed into a release image.
pub(crate) fn loader_manifest_matches(root: &Path, arch: &str) -> bool {
    let Some(manifest) = read_loader_manifest(root, arch) else {
        return false;
    };
    let loader = root.join("build").join(arch).join("ld-elf.so.1");
    let Ok(bytes) = std::fs::read(&loader) else {
        return false;
    };
    manifest.arch == arch
        && manifest.image_base == format!("0x{:x}", LOADER_IMAGE_BASE)
        && manifest.interpreter == LOADER_INTERPRETER
        && manifest.size == bytes.len() as u64
        && manifest.crc32 == format!("{:08x}", raven_initfs::crc32(&bytes))
}

/// True when the linked `libc.so.1`/loader DSO stamp matches the current sources and files.
///
/// The loader manifest above is the fast reuse check for the loader file; the DSO stamp is
/// the source gate. A `libc-pack` of changed static sources does not relink the dynamic pair,
/// so the image path must compare this stamp before staging `libc.so.1`/`ld-elf.so.1`.
pub(crate) fn dso_stamp_current(root: &Path, arch: &str) -> bool {
    let args = vec![
        "--arch".to_string(),
        arch.to_string(),
        "--verify".to_string(),
    ];
    let Ok(options) = parse(&args) else {
        return false;
    };
    verify_dso_stamp(root, &options).is_ok()
}

/// Run the K7 part of the mkmap check against the linked `libc.so.1`.
fn check_mkmap(
    root: &Path,
    options: &Options,
    archive: &Path,
    list: &Path,
    map: &Path,
    libc: &Path,
) -> Result<String> {
    crate::libc_pack_tools::mkmap(
        root,
        &[
            "check".to_string(),
            "--archive".to_string(),
            archive.display().to_string(),
            "--list".to_string(),
            list.display().to_string(),
            "--map".to_string(),
            map.display().to_string(),
            "--versions".to_string(),
            root.join("libc/ravenlibc/abi/Versions.def")
                .display()
                .to_string(),
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
            "--so".to_string(),
            libc.display().to_string(),
        ],
    )
}

fn find_linker(_root: &Path) -> Result<PathBuf> {
    if let Ok(value) = std::env::var("LD_LLD")
        && !value.is_empty()
    {
        return Ok(PathBuf::from(value));
    }
    if let Ok(output) = Command::new("rustc").arg("--print").arg("sysroot").output()
        && output.status.success()
    {
        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let host = "x86_64-unknown-linux-gnu";
        let candidate = PathBuf::from(root)
            .join("lib/rustlib")
            .join(host)
            .join("bin/rust-lld");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("libc-link: ld.lld not found (rust-lld or LD_LLD)")
}

fn write_undef(symbols: &Path, out: &Path) -> Result<()> {
    let text = std::fs::read_to_string(symbols)
        .with_context(|| format!("cannot read {}", symbols.display()))?;
    let mut response = String::new();
    let mut block_comment = false;
    for line in text.lines() {
        let line = line.trim();
        if block_comment {
            if line.contains("*/") {
                block_comment = false;
            }
            continue;
        }
        if line.starts_with("/*") {
            block_comment = !line.contains("*/");
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            if line == "# private" {
                break;
            }
            continue;
        }
        response.push_str("-u ");
        response.push_str(line);
        response.push('\n');
    }
    std::fs::write(out, response).with_context(|| format!("cannot write {}", out.display()))
}

/// The compiler-builtins archive carries hidden definitions of several public names (`mem*`,
/// `strlen`, and the soft-float math functions). If the linker resolves one from such a member,
/// the public symbol becomes `LOCAL HIDDEN` and K7 fails. Pass the shell members that define the
/// public `Symbols.list` names explicitly, before the archive, so the public definition wins.
fn forced_members(
    root: &Path,
    options: &Options,
    archive: &Path,
    symbols: &Path,
    out: &Path,
) -> Result<Vec<PathBuf>> {
    let symbol_text = std::fs::read_to_string(symbols)
        .with_context(|| format!("cannot read {}", symbols.display()))?;
    let mut public = BTreeSet::new();
    for line in symbol_text.lines() {
        let line = line.trim();
        if line == "# private" {
            break;
        }
        if !line.is_empty() && !line.starts_with('#') {
            public.insert(line.to_string());
        }
    }
    let tools = crate::libc_pack_tools::Tools::discover(root, options.arch)?;
    let output = Command::new(&tools.nm)
        .args(["-A", "--defined-only"])
        .arg(archive)
        .output()
        .with_context(|| format!("cannot run {}", tools.nm.display()))?;
    if !output.status.success() {
        bail!("libc-link: nm failed for {}", archive.display());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut members = BTreeSet::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 4 {
            continue;
        }
        let Some(symbol) = tokens.get(3) else {
            continue;
        };
        if symbol.is_empty() || !public.contains(*symbol) || tokens.get(2) != Some(&"T") {
            continue;
        }
        let Some((location, _value)) = tokens[0].rsplit_once(':') else {
            continue;
        };
        let Some((_archive, member)) = location.rsplit_once(':') else {
            continue;
        };
        if !member.contains("compiler_builtins") {
            members.insert(member.to_string());
        }
    }
    if members.is_empty() {
        bail!("libc-link: no shell members define the public symbol list");
    }
    let directory = out.join("link/forced");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let mut paths = Vec::new();
    for member in members {
        let path = directory.join(format!("{member}.o"));
        let file =
            File::create(&path).with_context(|| format!("cannot create {}", path.display()))?;
        let status = Command::new(&tools.ar)
            .arg("p")
            .arg(archive)
            .arg(&member)
            .stdout(Stdio::from(file))
            .status()
            .with_context(|| format!("cannot run {}", tools.ar.display()))?;
        if !status.success() {
            bail!(
                "libc-link: cannot extract {member} from {}",
                archive.display()
            );
        }
        paths.push(path);
    }
    Ok(paths)
}

#[allow(clippy::too_many_arguments)] // the link inputs are clearer as explicit paths
fn link_libc(
    root: &Path,
    options: &Options,
    linker: &Path,
    archive: &Path,
    symbols: &Path,
    version_script: &Path,
    undef: &Path,
    output: &Path,
) -> Result<bool> {
    let forced = forced_members(
        root,
        options,
        archive,
        symbols,
        output
            .parent()
            .context("libc.so.1 has no parent directory")?,
    )?;
    let mut args = vec![
        LINKER_FLAVOR.to_string(),
        "gnu".to_string(),
        "-shared".to_string(),
        "--image-base=0x100000".to_string(),
        "--soname=libc.so.1".to_string(),
        "-z".to_string(),
        "now".to_string(),
        "-z".to_string(),
        "relro".to_string(),
        "-z".to_string(),
        "nodelete".to_string(),
        "-z".to_string(),
        "noexecstack".to_string(),
        "-z".to_string(),
        "max-page-size=4096".to_string(),
        "-z".to_string(),
        "common-page-size=4096".to_string(),
        "-z".to_string(),
        "pack-relative-relocs".to_string(),
        "--gc-sections".to_string(),
        "--hash-style=gnu".to_string(),
        "-Bsymbolic-non-weak-functions".to_string(),
        "--version-script".to_string(),
        version_script.display().to_string(),
        "--dynamic-list".to_string(),
        root.join("libc/ravenlibc/ld/interpose.list")
            .display()
            .to_string(),
        format!("@{}", undef.display()),
        "-o".to_string(),
        output.display().to_string(),
    ];
    if options.arch == Arch::Aarch64 {
        // C74id: aarch64 DSO only. ICF folds duplicate function bodies; strip-debug keeps the
        // debug sections out of the F budget. The exported symbol set and version script are
        // unchanged.
        args.push("--icf=all".to_string());
        args.push("--strip-debug".to_string());
        args.push("--discard-all".to_string());
    }
    for path in &forced {
        args.push(path.display().to_string());
    }
    args.push(archive.display().to_string());
    args.insert(3, "-z".to_string());
    args.insert(4, "defs".to_string());
    let first = run_link(linker, &args, root)?;
    if first == 0 {
        return Ok(true);
    }
    println!("LINK-FINDING: libc.so.1 needs undefined symbols; retrying without -z defs");
    let filtered: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(index, _)| *index < 3 || *index >= 5)
        .map(|(_, value)| value.clone())
        .collect();
    let second = run_link(linker, &filtered, root)?;
    let _ = options;
    Ok(second == 0)
}

fn link_loader(root: &Path, options: &Options, linker: &Path, output: &Path) -> Result<bool> {
    let target = options.arch.triple();
    let target_dir = root.join(&options.out).join("cargo/libc-rtld");
    let status = run_command(
        &CommandSpec::new(
            "cargo",
            vec![
                "rustc".into(),
                "-p".into(),
                "raven-libc-rtld".into(),
                "--crate-type".into(),
                "staticlib".into(),
                "--profile".into(),
                "libc-so".into(),
                "--target".into(),
                target.into(),
                "--target-dir".into(),
                target_dir.display().to_string(),
            ],
            root,
        )
        .env("RUSTFLAGS", "-C relocation-model=pic"),
    )?;
    if status != 0 {
        println!("libc-link: ld-elf.so.1: FAILED (cargo rustc exit {status})");
        return Ok(false);
    }
    let archive = target_dir
        .join(target)
        .join("libc-so")
        .join("libraven_libc_rtld.a");
    if !archive.is_file() {
        println!(
            "libc-link: ld-elf.so.1: FAILED (archive missing: {})",
            archive.display()
        );
        return Ok(false);
    }
    let mut args = vec![
        LINKER_FLAVOR.to_string(),
        "gnu".to_string(),
        "-shared".to_string(),
        "--image-base=0x1000".to_string(),
        "-e".to_string(),
        "_rtld_start".to_string(),
        "-Bsymbolic".to_string(),
        "--no-undefined".to_string(),
        "-z".to_string(),
        "now".to_string(),
        "-z".to_string(),
        "relro".to_string(),
        "-z".to_string(),
        "noexecstack".to_string(),
        "-z".to_string(),
        "pack-relative-relocs".to_string(),
        "-z".to_string(),
        "max-page-size=4096".to_string(),
        "-z".to_string(),
        "common-page-size=4096".to_string(),
        "--hash-style=gnu".to_string(),
        "--gc-sections".to_string(),
        "--soname=ld-elf.so.1".to_string(),
        "--version-script".to_string(),
        root.join("libc/ravenlibc/ld/Symbols.map")
            .display()
            .to_string(),
        "-u".to_string(),
        "_rtld_start".to_string(),
    ];
    for name in export_names() {
        args.push("-u".to_string());
        args.push(name.to_string());
    }
    args.push(archive.display().to_string());
    args.push("-o".to_string());
    args.push(output.display().to_string());
    let status = run_link(linker, &args, root)?;
    Ok(status == 0)
}

fn run_link(linker: &Path, args: &[String], root: &Path) -> Result<i32> {
    let status = Command::new(linker)
        .args(args)
        .current_dir(root)
        .status()
        .with_context(|| format!("cannot run {}", linker.display()))?;
    Ok(status.code().unwrap_or(1))
}

fn export_names() -> [&'static str; 17] {
    [
        "_rtld_error_str",
        "dlclose",
        "dlerror",
        "dlopen",
        "dlsym",
        "dlvsym",
        "dladdr",
        "dlinfo",
        "dl_iterate_phdr",
        "__tls_get_addr",
        "_rtld_allocate_tls",
        "_rtld_free_tls",
        "_rtld_get_stack_prot",
        "_rtld_set_dlerror_slot",
        "r_debug_state",
        "_r_debug_postinit",
        "_rtld_version",
    ]
}

fn readelf(path: &Path) -> Result<String> {
    let output = Command::new("readelf")
        .args(["-hldrsW", path.to_str().unwrap_or_default()])
        .output()
        .context("cannot run readelf")?;
    if !output.status.success() {
        bail!("readelf failed for {}", path.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn hex(value: &str) -> Option<u64> {
    let digits = value.trim();
    let digits = digits.strip_prefix("0x").unwrap_or(digits);
    u64::from_str_radix(digits, 16).ok()
}

/// The ELF gate over a `readelf -hldrsW` dump.
pub(crate) fn gate_text(text: &str, kind: &str) -> Result<Vec<String>> {
    let mut findings = Vec::new();
    let mut fail = |message: String| findings.push(message);
    let is_dyn = text
        .lines()
        .any(|line| line.trim().starts_with("Type:") && line.contains("DYN"));
    if !is_dyn {
        fail("not an ET_DYN object".to_string());
    }

    let mut load_count = 0usize;
    let mut first_load_vaddr = None;
    let mut stack_seen = false;
    for line in text.lines() {
        let trimmed = line.trim();
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        match tokens.first().copied() {
            Some("LOAD") => {
                load_count = load_count.saturating_add(1);
                if first_load_vaddr.is_none() {
                    first_load_vaddr = tokens.get(2).and_then(|value| hex(value));
                }
                let align = tokens.last().and_then(|value| hex(value));
                if align != Some(0x1000) {
                    fail(format!("PT_LOAD alignment is not 4096: {trimmed}"));
                }
                if tokens.len() > 7 {
                    let flags = &tokens[6..tokens.len().saturating_sub(1)];
                    let write = flags.iter().any(|flag| flag.contains('W'));
                    let execute = flags.iter().any(|flag| flag.contains('E'));
                    if write && execute {
                        fail(format!("W|X load segment: {trimmed}"));
                    }
                }
            }
            Some("INTERP") => fail(format!("{kind} has PT_INTERP")),
            Some("TLS") => fail(format!("{kind} has PT_TLS")),
            Some("GNU_STACK") => {
                stack_seen = true;
                if tokens.len() > 7 {
                    let flags = &tokens[6..tokens.len().saturating_sub(1)];
                    if flags.iter().any(|flag| flag.contains('E')) {
                        fail("PT_GNU_STACK is executable".to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if load_count == 0 {
        fail("no PT_LOAD segment".to_string());
    }
    if load_count > 16 {
        fail(format!("{load_count} PT_LOAD segments (maximum 16)"));
    }
    if !text.contains("GNU_RELRO") {
        fail("missing PT_GNU_RELRO".to_string());
    }
    if !stack_seen {
        fail("missing PT_GNU_STACK".to_string());
    }
    if text.contains("TEXTREL") {
        fail("TEXTREL present".to_string());
    }
    let bind_now = text.lines().any(|line| {
        (line.contains("(FLAGS)") && line.contains("BIND_NOW"))
            || (line.contains("FLAGS_1") && line.contains("NOW"))
    });
    if !bind_now {
        fail("BIND_NOW is not set".to_string());
    }
    if text.contains("(NEEDED)") {
        fail(format!("{kind} has DT_NEEDED"));
    }
    if kind == "libc" && !text.contains("Library soname: [libc.so.1]") {
        fail("libc SONAME is not libc.so.1".to_string());
    }
    if kind == "loader" {
        if first_load_vaddr != Some(LOADER_IMAGE_BASE) {
            fail(format!(
                "loader first PT_LOAD image base is not 0x{LOADER_IMAGE_BASE:x} (found {first_load_vaddr:?})"
            ));
        }
        if !text.contains("Library soname: [ld-elf.so.1]") {
            fail("loader SONAME is not ld-elf.so.1".to_string());
        }
        let entry = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("Entry point address:"))
            .and_then(hex);
        let start = text.lines().find_map(|line| {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.last() == Some(&"_rtld_start") {
                tokens.get(1).and_then(|value| hex(value))
            } else {
                None
            }
        });
        match (entry, start) {
            (Some(entry), Some(start)) if entry != 0 && entry == start => {}
            _ => fail("loader entry point is not _rtld_start".to_string()),
        }
        for line in text.lines() {
            if (line.contains("R_X86_64_")
                || line.contains("R_AARCH64_")
                || line.contains("R_RISCV_"))
                && !(line.contains("RELATIVE") || line.contains("IRELATIVE"))
            {
                fail(format!(
                    "loader has a non-relative relocation: {}",
                    line.trim()
                ));
            }
        }
        let mut exported = BTreeSet::new();
        let mut in_dynsym = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Symbol table '.dynsym'") {
                in_dynsym = true;
                continue;
            }
            if trimmed.starts_with("Symbol table '") {
                in_dynsym = false;
                continue;
            }
            if !in_dynsym {
                continue;
            }
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            if tokens.len() < 8 || !tokens[0].ends_with(':') {
                continue;
            }
            if tokens[4] != "GLOBAL" || tokens[6] == "UND" {
                continue;
            }
            if let Some((name, _version)) = tokens[7].split_once("@@") {
                exported.insert(name.to_string());
            }
        }
        for name in export_names() {
            if !exported.contains(name) {
                fail(format!("loader does not export {name}"));
            }
        }
        if exported.len() != export_names().len() {
            fail(format!(
                "loader exports {} symbols, expected 17",
                exported.len()
            ));
        }
    }
    if findings.is_empty() {
        Ok(vec![format!("{kind}: ELF gate ok")])
    } else {
        bail!("{kind}: {}", findings.join("; "))
    }
}

/// The ELF gate.
pub(crate) fn check_elf(path: &Path, kind: &str) -> Result<Vec<String>> {
    let text = readelf(path)?;
    gate_text(&text, kind)
}

#[cfg(test)]
#[path = "libc_link_ut.rs"]
mod tests;
