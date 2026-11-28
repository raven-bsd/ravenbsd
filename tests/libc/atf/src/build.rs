// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Compiling donor test programs twice: the host oracle and the ravenlibc target.
//!
//! The target link is driven by hand because clang's own link step for an unknown OS falls back to
//! the host driver; see C71 4.6. Every failure is classified from the compiler's stderr so the
//! table can name a missing header or symbol instead of a generic error.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::suite::Program;
use crate::{BuildStatus, Framework, Mode};

/// Directory of ATF's own sources inside `vendor/atf`.
const ATF_SOURCES: &[&str] = &["atf-c", "atf-c/detail"];
/// Weak target diagnostics the C donor tests call but the current C ABI does not export.
const TARGET_SHIM: &str = "tests/libc/atf/shim/rl_target_shim.c";

/// Builds programs in one mode.
#[derive(Clone, Debug)]
pub struct Builder {
    /// Repository root.
    pub root: PathBuf,
    /// Vendored donor root (`tests/libc-oracle/vendor`).
    pub vendor: PathBuf,
    /// Build root of the exact embedded test-port sources (`out/port-sources/build`).
    pub port_root: PathBuf,
    /// Output root (`build/libc-conformance`).
    pub out: PathBuf,
    /// Oracle or target.
    pub mode: Mode,
    /// C compiler command (default `cc`; target always `clang`).
    pub cc: String,
    /// Sysroot for target mode.
    pub sysroot: Option<PathBuf>,
}

/// Outcome of building one program.
#[derive(Clone, Debug)]
pub struct BuildReport {
    /// Status recorded in the suite run.
    pub status: BuildStatus,
    /// Compiler output (truncated for the log file in the caller).
    pub log: String,
    /// Every distinct missing header.
    pub missing_headers: BTreeSet<String>,
    /// Every distinct missing symbol.
    pub missing_symbols: BTreeSet<String>,
    /// The built executable, when successful.
    pub binary: Option<PathBuf>,
}

impl BuildReport {
    fn failed(reason: String, log: String) -> Self {
        Self {
            status: BuildStatus::Failed(reason),
            log,
            missing_headers: BTreeSet::new(),
            missing_symbols: BTreeSet::new(),
            binary: None,
        }
    }
}

impl Builder {
    /// Build the `libatf-c.a` archive once per mode.
    pub fn build_atf(&self) -> Result<PathBuf> {
        let atf = self.vendor.join("vendor/atf");
        let directory = self.out.join("libatf-c").join(self.mode.as_str());
        let include = directory.join("include");
        fs::create_dir_all(include.join("atf-c"))
            .with_context(|| format!("cannot create {}", include.display()))?;
        let config = self.root.join("tests/libc/atf/config");
        let bconfig = match self.mode {
            Mode::Oracle => config.join("host_bconfig.h"),
            Mode::Target => config.join("target_bconfig.h"),
        };
        fs::copy(&bconfig, include.join("bconfig.h"))
            .with_context(|| format!("cannot install {}", bconfig.display()))?;
        fs::copy(config.join("defs.h"), include.join("atf-c/defs.h"))
            .with_context(|| "cannot install defs.h")?;
        let archive = directory.join("libatf-c.a");
        let objects = directory.join("obj");
        fs::create_dir_all(&objects)
            .with_context(|| format!("cannot create {}", objects.display()))?;
        let mut built = Vec::new();
        for source in atf_sources(&atf)? {
            let object = objects.join(format!(
                "{}.o",
                source
                    .with_extension("")
                    .to_string_lossy()
                    .replace('/', "_")
            ));
            self.compile_atf_source(&atf, &include, &atf.join(&source), &object)?;
            built.push(object);
        }
        let mut args = vec!["rcs".to_string(), archive.display().to_string()];
        args.extend(built.iter().map(|path| path.display().to_string()));
        run_checked("ar", &args, self.root())?;
        Ok(archive)
    }

    fn compile_atf_source(
        &self,
        atf: &Path,
        include: &Path,
        source: &Path,
        object: &Path,
    ) -> Result<()> {
        let shim = self.root.join("tests/libc/atf/shim/rl_shim.h");
        let mut args = self.common_compile_args(&shim, include);
        args.extend([
            "-I".to_string(),
            atf.display().to_string(),
            "-I".to_string(),
            atf.join("atf-c").display().to_string(),
        ]);
        match self.mode {
            Mode::Oracle => {}
            Mode::Target => {
                let sysroot = self.sysroot()?;
                args.extend([
                    "--target=x86_64-unknown-ravenbsd".to_string(),
                    format!("--sysroot={}", sysroot.display()),
                    "-I".to_string(),
                    sysroot.join("usr/include").display().to_string(),
                    "-I".to_string(),
                    self.root
                        .join("tests/libc/atf/config")
                        .display()
                        .to_string(),
                ]);
            }
        }
        args.extend(atf_string_definitions(self.mode));
        args.extend([
            "-c".to_string(),
            source.display().to_string(),
            "-o".to_string(),
            object.display().to_string(),
        ]);
        let program = self.atf_compiler();
        run_checked(&program, &args, self.root())
            .with_context(|| format!("cannot compile atf-c source {}", source.display()))?;
        Ok(())
    }

    /// Build one program in the configured mode.
    pub fn build(
        &self,
        suite_name: &str,
        suite_root: &str,
        framework: Framework,
        program: &Program,
    ) -> Result<BuildReport> {
        match self.mode {
            Mode::Oracle => self.build_oracle(suite_name, suite_root, framework, program),
            Mode::Target => self.build_target(suite_name, suite_root, framework, program),
        }
    }

    fn build_oracle(
        &self,
        suite_name: &str,
        suite_root: &str,
        framework: Framework,
        program: &Program,
    ) -> Result<BuildReport> {
        let root = self.suite_root(suite_root);
        let binary = self.out.join("oracle").join(suite_name).join(&program.name);
        let Some(parent) = binary.parent() else {
            bail!("program name has no parent: {}", program.name);
        };
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
        let archive = self.out.join("libatf-c/oracle/libatf-c.a");
        if framework == Framework::Atf && !archive.is_file() {
            self.build_atf()?;
        }
        let shim = self.root.join("tests/libc/atf/shim/rl_shim.h");
        let include = self.out.join("libatf-c/oracle/include");
        let mut args = vec![
            "-std=gnu11".to_string(),
            "-O1".to_string(),
            "-w".to_string(),
        ];
        args.extend(self.common_compile_args(&shim, &include));
        args.extend(["-I".to_string(), root.display().to_string()]);
        for source in &program.sources {
            if let Some(parent) = root.join(source).parent() {
                args.push("-I".to_string());
                args.push(parent.display().to_string());
            }
        }
        args.extend([
            "-I".to_string(),
            self.vendor.join("vendor/atf").display().to_string(),
            "-I".to_string(),
            self.vendor.join("vendor/atf/atf-c").display().to_string(),
        ]);
        args.extend(program.cflags.iter().cloned());
        args.extend(
            program
                .sources
                .iter()
                .map(|source| root.join(source).display().to_string()),
        );
        if framework == Framework::Atf {
            args.push(archive.display().to_string());
        }
        args.extend(program.ldadd.iter().cloned());
        args.extend(["-o".to_string(), binary.display().to_string()]);
        let output = run(self.compiler(), &args, self.root())?;
        Ok(self.report(output, Some(binary)))
    }

    fn build_target(
        &self,
        suite_name: &str,
        suite_root: &str,
        framework: Framework,
        program: &Program,
    ) -> Result<BuildReport> {
        let sysroot = self.sysroot()?;
        let root = self.suite_root(suite_root);
        let directory = self.out.join("target").join(suite_name).join(&program.name);
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        let binary = directory.join("program");
        let objects = directory.join("obj");
        fs::create_dir_all(&objects)
            .with_context(|| format!("cannot create {}", objects.display()))?;
        let shim = self.root.join("tests/libc/atf/shim/rl_shim.h");
        let include = self.out.join("libatf-c/target/include");
        let mut missing_headers = BTreeSet::new();
        let mut missing_symbols = BTreeSet::new();
        let mut log = String::new();
        let mut object_list = Vec::new();
        for source in &program.sources {
            let object = objects.join(format!(
                "{}.o",
                Path::new(source)
                    .with_extension("")
                    .to_string_lossy()
                    .replace('/', "_")
            ));
            let mut args = vec![
                "--target=x86_64-unknown-ravenbsd".to_string(),
                format!("--sysroot={}", sysroot.display()),
                "-std=gnu11".to_string(),
                "-O1".to_string(),
                "-w".to_string(),
                "-D_GNU_SOURCE".to_string(),
                "-include".to_string(),
                shim.display().to_string(),
                "-include".to_string(),
                self.shim_source("rl_target_prelude.h")
                    .display()
                    .to_string(),
                format!("-I{}", self.root.join("tests/libc/atf/shim").display()),
                format!(
                    "-I{}",
                    self.root.join("tests/libc/atf/shim/netbsd").display()
                ),
                format!("-I{}", sysroot.join("usr/include").display()),
                format!("-I{}", root.display()),
                "-I".to_string(),
                include.display().to_string(),
                format!("-I{}", self.vendor.join("vendor/atf").display()),
                format!("-I{}", self.vendor.join("vendor/atf/atf-c").display()),
                "-DHAVE_CONFIG_H".to_string(),
            ];
            if let Some(parent) = root.join(source).parent() {
                args.push("-I".to_string());
                args.push(parent.display().to_string());
            }
            args.extend(program.cflags.iter().cloned());
            args.extend([
                "-c".to_string(),
                root.join(source).display().to_string(),
                "-o".to_string(),
                object.display().to_string(),
            ]);
            let output = run(self.target_compiler(), &args, self.root())?;
            log.push_str(&output.text);
            if !output.success {
                classify_into(&output.text, &mut missing_headers, &mut missing_symbols);
                return Ok(self.classified(output, log));
            }
            object_list.push(object);
        }
        if framework == Framework::Atf {
            let archive = self.out.join("libatf-c/target/libatf-c.a");
            if !archive.is_file() {
                self.build_atf()?;
            }
        }
        object_list.push(self.target_shim()?);
        let args = target_link_args(
            sysroot,
            &binary,
            &object_list,
            framework,
            &self.out,
            &program.ldadd,
        );
        let output = run(self.linker()?, &args, self.root())?;
        log.push_str(&output.text);
        if output.success {
            Ok(BuildReport {
                status: BuildStatus::Built,
                log,
                missing_headers,
                missing_symbols,
                binary: Some(binary),
            })
        } else {
            classify_into(&output.text, &mut missing_headers, &mut missing_symbols);
            let mut report = self.classified(output, log);
            report.missing_headers = missing_headers;
            report.missing_symbols = missing_symbols;
            Ok(report)
        }
    }

    /// Compile the weak target shim once per output tree.
    fn target_shim(&self) -> Result<PathBuf> {
        let sysroot = self.sysroot()?;
        let directory = self.out.join("target-shim");
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        let object = directory.join("rl_target_shim.o");
        let source = self.root.join(TARGET_SHIM);
        let args = target_shim_args(sysroot, &source, &object);
        let output = run(self.target_compiler(), &args, self.root())?;
        if !output.success {
            let line = output
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("no output");
            bail!("cannot compile the target shim: {line}");
        }
        Ok(object)
    }

    /// Build the target wrapper that runs a donor body from a writable directory with the login
    /// group list. The wrapper is a host-side runner artifact, never part of a shipped image.
    pub fn build_target_wrapper(&self) -> Result<PathBuf> {
        let sysroot = self.sysroot()?;
        let directory = self.out.join("target-shim");
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        let object = directory.join("atf_wrapper.o");
        let binary = directory.join("atf-wrapper");
        let source = self.shim_source("atf_wrapper.c");
        let output = run(
            self.target_compiler(),
            &target_shim_args(sysroot, &source, &object),
            self.root(),
        )?;
        if !output.success {
            let line = output
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("no output");
            bail!("cannot compile the target wrapper: {line}");
        }
        let args = target_link_args(
            sysroot,
            &binary,
            &[object],
            Framework::ExitStatus,
            &self.out,
            &[],
        );
        let output = run(self.linker()?, &args, self.root())?;
        if !output.success {
            let line = output
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("no output");
            bail!("cannot link the target wrapper: {line}");
        }
        Ok(binary)
    }

    /// Build the vendored pjdfstest helper for the target.
    ///
    /// The helper is the donor `pjdfstest.c` plus the minimal `config.h` beside it. It needs no
    /// ATF shim and is not an ATF program: `open/00.t` invokes it once per expectation.
    pub fn build_tap_helper(&self) -> Result<PathBuf> {
        let sysroot = self.sysroot()?;
        let directory = self.out.join("tap");
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        let object = directory.join("pjdfstest.o");
        let binary = directory.join("pjdfstest");
        let source = self.vendor.join("vendor/pjdfstest/pjdfstest.c");
        let include = self.vendor.join("vendor/pjdfstest");
        if !source.is_file() {
            bail!("vendored pjdfstest helper missing: {}", source.display());
        }
        let args = vec![
            "--target=x86_64-unknown-ravenbsd".to_string(),
            format!("--sysroot={}", sysroot.display()),
            "-std=gnu11".to_string(),
            "-O1".to_string(),
            "-w".to_string(),
            "-D_GNU_SOURCE".to_string(),
            // ravenlibc declares __assert_fail but does not export it; the helper's two internal
            // asserts are not on the open/create path.
            "-DNDEBUG".to_string(),
            format!("-I{}", include.display()),
            "-c".to_string(),
            source.display().to_string(),
            "-o".to_string(),
            object.display().to_string(),
        ];
        let output = run(self.target_compiler(), &args, self.root())?;
        if !output.success {
            let line = output
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("no output");
            bail!("cannot compile the pjdfstest helper: {line}");
        }
        let mut args = target_link_args(
            sysroot,
            &binary,
            &[object],
            Framework::ExitStatus,
            &self.out,
            &[],
        );
        // The kernel's exec loader requires the page-aligned test layout (PT_PHDR in the first
        // LOAD); lld's default layout is rejected with ENOEXEC.
        args.push(format!(
            "--script={}",
            self.root.join("tests/libc/target/x86_64.ld").display()
        ));
        let output = run(self.linker()?, &args, self.root())?;
        if !output.success {
            let line = output
                .text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("no output");
            bail!("cannot link the pjdfstest helper: {line}");
        }
        Ok(binary)
    }

    fn report(&self, output: CommandOutput, binary: Option<PathBuf>) -> BuildReport {
        let mut missing_headers = BTreeSet::new();
        let mut missing_symbols = BTreeSet::new();
        if output.success {
            BuildReport {
                status: BuildStatus::Built,
                log: output.text,
                missing_headers,
                missing_symbols,
                binary,
            }
        } else {
            classify_into(&output.text, &mut missing_headers, &mut missing_symbols);
            let mut report = self.classified(output, String::new());
            report.missing_headers = missing_headers;
            report.missing_symbols = missing_symbols;
            report
        }
    }

    fn classified(&self, output: CommandOutput, log: String) -> BuildReport {
        let reason = classify(&output.text);
        BuildReport::failed(reason, format!("{log}{}", output.text))
    }

    fn common_compile_args(&self, shim: &Path, include: &Path) -> Vec<String> {
        vec![
            "-D_GNU_SOURCE".to_string(),
            "-include".to_string(),
            shim.display().to_string(),
            "-I".to_string(),
            self.vendor.join("vendor/atf").display().to_string(),
            "-I".to_string(),
            self.vendor.join("vendor/atf/atf-c").display().to_string(),
            "-I".to_string(),
            include.display().to_string(),
            "-DHAVE_CONFIG_H".to_string(),
        ]
    }

    fn compiler(&self) -> String {
        if self.cc.trim().is_empty() {
            "cc".to_string()
        } else {
            self.cc.clone()
        }
    }

    /// The compiler for the ATF sources themselves: the target compiler in target mode,
    /// because a host `cc` rejects `--target=x86_64-unknown-ravenbsd`.
    fn atf_compiler(&self) -> String {
        if self.mode == Mode::Target {
            self.target_compiler()
        } else {
            self.compiler()
        }
    }

    fn target_compiler(&self) -> String {
        if self.cc.trim().is_empty() {
            "clang".to_string()
        } else {
            self.cc.clone()
        }
    }

    /// The linker for the ravenbsd target.
    ///
    /// clang's driver for `x86_64-unknown-ravenbsd` falls back to the host gcc `collect2`, which
    /// cannot find a linker for the target even with `-fuse-ld=lld`. Link with lld directly:
    /// `RAVEN_LLD` wins (a rust-lld path is convenient), then the `rust-lld` beside the active
    /// toolchain, then `ld.lld` from `PATH`.
    fn linker(&self) -> Result<String> {
        if let Ok(path) = std::env::var("RAVEN_LLD") {
            if Path::new(&path).is_file() {
                return Ok(path);
            }
            bail!("RAVEN_LLD is not a file: {path}");
        }
        if let Some(path) = rust_lld() {
            return Ok(path.display().to_string());
        }
        if Command::new("ld.lld")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Ok("ld.lld".to_string());
        }
        bail!("target link needs lld: set RAVEN_LLD to rust-lld or install ld.lld")
    }

    fn root(&self) -> &Path {
        &self.root
    }

    /// The source root for a suite: the staged exact port when present, else the vendored tree.
    fn suite_root(&self, suite_root: &str) -> PathBuf {
        let staged = self.port_root.join("tests/libc-oracle").join(suite_root);
        if staged.is_dir() {
            staged
        } else {
            self.vendor.join(suite_root)
        }
    }

    /// A test-port shim source staged from the embedded text, else the tracked shim.
    fn shim_source(&self, name: &str) -> PathBuf {
        let staged = self.port_root.join("tests/libc/atf/shim").join(name);
        if staged.is_file() {
            staged
        } else {
            self.root.join("tests/libc/atf/shim").join(name)
        }
    }

    fn sysroot(&self) -> Result<&Path> {
        let Some(sysroot) = &self.sysroot else {
            bail!("target mode needs --sysroot");
        };
        Ok(sysroot)
    }
}

/// The sysroot files C60/C37/C61 must install before a target build can start.
pub const SYSROOT_REQUIRED: &[&str] = &[
    "usr/include/stdio.h",
    "usr/lib/libc.a",
    "usr/lib/crt1.o",
    "usr/lib/crti.o",
    "usr/lib/crtn.o",
];

/// Return the first missing required sysroot path.
#[must_use]
pub fn sysroot_missing(sysroot: &Path) -> Option<PathBuf> {
    SYSROOT_REQUIRED
        .iter()
        .map(|path| sysroot.join(path))
        .find(|path| !path.exists())
}

/// Classify a compiler failure.
#[must_use]
pub fn classify(stderr: &str) -> String {
    let mut headers = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    classify_into(stderr, &mut headers, &mut symbols);
    if let Some(header) = headers.first() {
        return format!("missing-header:{header}");
    }
    if let Some(symbol) = symbols.first() {
        return format!("missing-symbol:{symbol}");
    }
    let line = stderr
        .lines()
        .find(|line| line.contains("error:") || line.contains("undefined reference"))
        .or_else(|| stderr.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("compiler failed with no output");
    format!("build-other:{}", line.trim())
}

/// Collect every missing header and symbol from compiler stderr.
pub fn classify_into(stderr: &str, headers: &mut BTreeSet<String>, symbols: &mut BTreeSet<String>) {
    for line in stderr.lines() {
        if let Some(rest) = line.split("fatal error: ").nth(1) {
            let rest = rest.trim();
            // clang's error limit line is not a header; the real fatal error precedes it.
            if !rest.starts_with("too many errors") {
                let quoted = rest
                    .strip_prefix('\'')
                    .and_then(|value| value.split('\'').next());
                let header = quoted.unwrap_or_else(|| rest.split(':').next().unwrap_or(rest));
                let header = header.trim().trim_matches(['\'', '"', '`']);
                if !header.is_empty() {
                    headers.insert(header.to_string());
                }
            }
        }
        for marker in [
            "undefined reference to ",
            "implicit declaration of function ",
            "undefined symbol: ",
        ] {
            if let Some(rest) = line.split(marker).nth(1) {
                let symbol: String = rest
                    .trim_start_matches(['`', '‘', '\''])
                    .chars()
                    .take_while(|character| {
                        !matches!(character, '\'' | '`' | '’' | ')' | ',' | ' ' | '\t')
                    })
                    .collect();
                if !symbol.is_empty() {
                    symbols.insert(symbol);
                }
            }
        }
    }
}

struct CommandOutput {
    success: bool,
    text: String,
}

fn run(program: String, args: &[String], cwd: &Path) -> Result<CommandOutput> {
    let output = Command::new(&program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("cannot run `{program}`"))?;
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    Ok(CommandOutput {
        success: output.status.success(),
        text,
    })
}

fn run_checked(program: &str, args: &[String], cwd: &Path) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("cannot run `{program}`"))?;
    if !output.status.success() {
        bail!(
            "`{program}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("")
        );
    }
    Ok(())
}

/// Locate `rust-lld` from the active toolchain.
fn rust_lld() -> Option<PathBuf> {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sysroot = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let entries = fs::read_dir(sysroot.join("lib/rustlib")).ok()?;
    for entry in entries.flatten() {
        let candidate = entry.path().join("bin/rust-lld");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Arguments that compile the weak target shim with the target headers.
fn target_shim_args(sysroot: &Path, source: &Path, object: &Path) -> Vec<String> {
    vec![
        "--target=x86_64-unknown-ravenbsd".to_string(),
        format!("--sysroot={}", sysroot.display()),
        "-std=gnu11".to_string(),
        "-O1".to_string(),
        "-w".to_string(),
        format!("-I{}", sysroot.join("usr/include").display()),
        "-c".to_string(),
        source.display().to_string(),
        "-o".to_string(),
        object.display().to_string(),
    ]
}

/// Arguments for the direct-lld target link (no clang driver in the path).
fn target_link_args(
    sysroot: &Path,
    binary: &Path,
    objects: &[PathBuf],
    framework: Framework,
    out: &Path,
    ldadd: &[String],
) -> Vec<String> {
    let mut args = vec![
        // `rust-lld` is the generic driver; the symlink names (`ld.lld`, `ld`) also accept it.
        "-flavor".to_string(),
        "gnu".to_string(),
        format!("--sysroot={}", sysroot.display()),
        "-static".to_string(),
        "-z".to_string(),
        "max-page-size=4096".to_string(),
        "--error-limit=0".to_string(),
        // Drop unreferenced libc sections: the kernel `PROC_EXEC` image limit is 1 MiB, and the
        // full static archive otherwise leaves an ATF program just over it.
        "--gc-sections".to_string(),
        "-o".to_string(),
        binary.display().to_string(),
        sysroot.join("usr/lib/crt1.o").display().to_string(),
        sysroot.join("usr/lib/crti.o").display().to_string(),
    ];
    args.extend(objects.iter().map(|path| path.display().to_string()));
    if framework == Framework::Atf {
        args.push(out.join("libatf-c/target/libatf-c.a").display().to_string());
    }
    args.extend(ldadd.iter().cloned());
    args.extend([
        "-L".to_string(),
        sysroot.join("usr/lib").display().to_string(),
        "-lc".to_string(),
        sysroot.join("usr/lib/crtn.o").display().to_string(),
    ]);
    args
}

fn atf_sources(atf: &Path) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    for directory in ATF_SOURCES {
        let path = atf.join(directory);
        for entry in
            fs::read_dir(&path).with_context(|| format!("cannot read {}", path.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", path.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".c") {
                continue;
            }
            if name.ends_with("_test.c")
                || name.contains("helper")
                || name.starts_with("h_")
                || name == "atf_c_test.c"
            {
                continue;
            }
            sources.push(PathBuf::from(directory).join(name));
        }
    }
    sources.sort();
    Ok(sources)
}

/// Build-time ATF path macros; equivalent to the NetBSD `Makefile.inc` flags.
fn atf_string_definitions(mode: Mode) -> Vec<String> {
    let machine = match mode {
        Mode::Oracle => "x86_64",
        Mode::Target => "ravenbsd",
    };
    let mut definitions = BTreeMap::new();
    definitions.insert("ATF_ARCH", machine);
    definitions.insert("ATF_BUILD_CC", "/usr/bin/cc");
    definitions.insert("ATF_BUILD_CFLAGS", "");
    definitions.insert("ATF_BUILD_CPP", "/usr/bin/cpp");
    definitions.insert("ATF_BUILD_CPPFLAGS", "");
    definitions.insert("ATF_BUILD_CXX", "/usr/bin/c++");
    definitions.insert("ATF_BUILD_CXXFLAGS", "");
    definitions.insert("ATF_CONFDIR", "/etc/atf");
    definitions.insert("ATF_INCLUDEDIR", "/usr/include");
    definitions.insert("ATF_LIBDIR", "/usr/lib");
    definitions.insert("ATF_LIBEXECDIR", "/usr/libexec");
    definitions.insert("ATF_MACHINE", "x86_64");
    definitions.insert("ATF_M4", "/usr/bin/m4");
    definitions.insert("ATF_PKGDATADIR", "/usr/share/atf");
    definitions.insert("ATF_SHELL", "/bin/sh");
    definitions.insert("ATF_WORKDIR", "/tmp");
    definitions
        .into_iter()
        .map(|(key, value)| format!("-D{key}=\"{value}\""))
        .collect()
}

#[cfg(test)]
#[path = "build_ut.rs"]
mod tests;
