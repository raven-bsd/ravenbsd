// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Locating the host clang and running one translation unit at a time.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::repo_root;

/// The target flavour used to spell `--target=`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TripleFlavour {
    /// `<arch>-unknown-ravenbsd` is accepted by the driver.
    Ravenbsd,
    /// Fallback: `<arch>-unknown-none-elf` (identical type macros, measured).
    NoneElf,
}

/// One of the three supported architectures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Arch {
    /// x86-64.
    X86_64,
    /// AArch64.
    Aarch64,
    /// RISC-V 64.
    Riscv64,
}

impl Arch {
    /// All architectures, in the canonical order.
    pub const ALL: [Self; 3] = [Self::X86_64, Self::Aarch64, Self::Riscv64];

    /// The directory name and target triple component.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }

    /// Parse a manifest/database name.
    pub fn from_name(name: &str) -> Result<Self, String> {
        match name {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::Aarch64),
            "riscv64" => Ok(Self::Riscv64),
            other => Err(format!("unknown architecture `{other}`")),
        }
    }

    /// The per-architecture include directory (absolute).
    #[must_use]
    pub fn include_dir(self) -> PathBuf {
        repo_root()
            .join("libc/ravenlibc/arch")
            .join(self.name())
            .join("include")
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A C or C++ language mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lang {
    /// ISO C 2011.
    C11,
    /// ISO C 2017.
    C17,
    /// ISO C 2023 (falls back to `c2x` on older clang).
    C23,
    /// GNU C 2017.
    Gnu17,
    /// ISO C++ 2017.
    Cxx17,
    /// ISO C++ 2020.
    Cxx20,
}

impl Lang {
    /// All matrix languages.
    pub const ALL: [Self; 6] = [
        Self::C11,
        Self::C17,
        Self::C23,
        Self::Gnu17,
        Self::Cxx17,
        Self::Cxx20,
    ];

    /// The quick-mode subset.
    pub const QUICK: [Self; 2] = [Self::C11, Self::Cxx17];

    /// True for the two C++ modes.
    #[must_use]
    pub fn is_cxx(self) -> bool {
        matches!(self, Self::Cxx17 | Self::Cxx20)
    }

    /// Short stable name used in diagnostics and `HDRCHECK_KEEP` file names.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::C11 => "c11",
            Self::C17 => "c17",
            Self::C23 => "c23",
            Self::Gnu17 => "gnu17",
            Self::Cxx17 => "cxx17",
            Self::Cxx20 => "cxx20",
        }
    }

    fn std_flag(self, toolchain: &Toolchain) -> &'static str {
        match self {
            Self::C11 => "c11",
            Self::C17 => "c17",
            Self::C23 => {
                if c23_supported(toolchain) {
                    "c23"
                } else {
                    "c2x"
                }
            }
            Self::Gnu17 => "gnu17",
            Self::Cxx17 => "c++17",
            Self::Cxx20 => "c++20",
        }
    }
}

/// The feature-visibility mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vis {
    /// No user macro (the documented default).
    Default,
    /// `-D_POSIX_C_SOURCE=200809L`.
    PosixStrict,
    /// `-D_XOPEN_SOURCE=700`.
    XopenStrict,
    /// `-D_GNU_SOURCE`.
    Gnu,
    /// `-D_ANSI_SOURCE`.
    Ansi,
}

impl Vis {
    /// All visibility modes.
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::PosixStrict,
        Self::XopenStrict,
        Self::Gnu,
        Self::Ansi,
    ];

    /// The quick-mode subset.
    pub const QUICK: [Self; 2] = [Self::Default, Self::PosixStrict];

    /// Stable name for probe rows and diagnostics.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::PosixStrict => "posix",
            Self::XopenStrict => "xopen",
            Self::Gnu => "gnu",
            Self::Ansi => "ansi",
        }
    }

    fn defines(self) -> &'static [&'static str] {
        match self {
            Self::Default => &[],
            Self::PosixStrict => &["-D_POSIX_C_SOURCE=200809L"],
            Self::XopenStrict => &["-D_XOPEN_SOURCE=700"],
            Self::Gnu => &["-D_GNU_SOURCE"],
            Self::Ansi => &["-D_ANSI_SOURCE"],
        }
    }
}

/// The located host compiler.
#[derive(Clone, Debug)]
pub struct Toolchain {
    /// Path or name of the clang executable.
    pub clang: PathBuf,
    /// The compiler's resource include directory.
    pub resource_include: PathBuf,
    /// The first line of `clang --version`.
    pub version: String,
    /// Which `--target` spelling the driver accepts.
    pub triple_flavour: TripleFlavour,
}

/// One translation unit fed on stdin.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tu {
    /// Target architecture.
    pub arch: Arch,
    /// Language mode.
    pub lang: Lang,
    /// Visibility mode.
    pub vis: Vis,
    /// The text fed on stdin.
    pub source: String,
}

/// The result of one clang invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// True when clang exited 0.
    pub ok: bool,
    /// Captured stderr (stdout is discarded).
    pub stderr: String,
}

static C23_SUPPORTED: OnceLock<bool> = OnceLock::new();

fn c23_supported(toolchain: &Toolchain) -> bool {
    *C23_SUPPORTED.get_or_init(|| {
        let status = Command::new(&toolchain.clang)
            .args(["-std=c23", "-E", "-x", "c", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(status) if status.success())
    })
}

/// Find clang (`RAVEN_CLANG` first) and probe the ravenbsd triple.
pub fn locate_toolchain() -> Result<Toolchain, String> {
    let clang = std::env::var_os("RAVEN_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let version = run_capture(&clang, &["--version"])?;
    let version = version
        .lines()
        .next()
        .unwrap_or("clang (unknown version)")
        .trim()
        .to_string();
    let resource = run_capture(&clang, &["-print-resource-dir"])?;
    let resource_include = PathBuf::from(resource.trim()).join("include");
    if !resource_include.is_dir() {
        return Err(format!(
            "clang resource include directory not found: {}",
            resource_include.display()
        ));
    }
    let ravenbsd_ok = Command::new(&clang)
        .args([
            "--target=x86_64-unknown-ravenbsd",
            "-E",
            "-dM",
            "-x",
            "c",
            "/dev/null",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    Ok(Toolchain {
        clang,
        resource_include,
        version,
        triple_flavour: if ravenbsd_ok {
            TripleFlavour::Ravenbsd
        } else {
            TripleFlavour::NoneElf
        },
    })
}

fn run_capture(program: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} {:?} failed: {}",
            program.display(),
            args,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn target(toolchain: &Toolchain, arch: Arch) -> String {
    match toolchain.triple_flavour {
        TripleFlavour::Ravenbsd => format!("--target={}-unknown-ravenbsd", arch.name()),
        TripleFlavour::NoneElf => format!("--target={}-unknown-none-elf", arch.name()),
    }
}

/// The exact argument vector of doc 14 section 4.2.
#[must_use]
pub fn clang_args(toolchain: &Toolchain, tu: &Tu) -> Vec<String> {
    let include = repo_root().join("libc/ravenlibc/include");
    let mut args = vec![
        target(toolchain, tu.arch),
        "-nostdinc".to_string(),
        "-isystem".to_string(),
        toolchain.resource_include.display().to_string(),
        "-isystem".to_string(),
        tu.arch.include_dir().display().to_string(),
        "-isystem".to_string(),
        include.display().to_string(),
        "-D__ravenbsd__=1".to_string(),
        "-D__unix__=1".to_string(),
        "-D__unix=1".to_string(),
        "-fsyntax-only".to_string(),
        "-Werror".to_string(),
        "-pedantic".to_string(),
        "-Wall".to_string(),
        "-Wextra".to_string(),
    ];
    if tu.lang.is_cxx() {
        args.extend(["-x".to_string(), "c++".to_string()]);
        args.push(format!("-std={}", tu.lang.std_flag(toolchain)));
        // clang 14 marks -nostdinc++ unused on riscv64 (no C++ include path
        // exists there), and -Werror turns that driver warning into an error.
        if tu.arch != Arch::Riscv64 {
            args.push("-nostdinc++".to_string());
        }
    } else {
        args.extend([
            "-x".to_string(),
            "c".to_string(),
            format!("-std={}", tu.lang.std_flag(toolchain)),
        ]);
    }
    args.extend(tu.vis.defines().iter().map(ToString::to_string));
    args.push("-".to_string());
    args
}

/// Compile one TU; a missing clang is an error the caller must fail on.
pub fn compile(toolchain: &Toolchain, tu: &Tu) -> Outcome {
    match compile_inner(toolchain, tu) {
        Ok(outcome) => outcome,
        Err(error) => Outcome {
            ok: false,
            stderr: error,
        },
    }
}

fn compile_inner(toolchain: &Toolchain, tu: &Tu) -> Result<Outcome, String> {
    keep_tu(tu);
    let mut child = Command::new(&toolchain.clang)
        .args(clang_args(toolchain, tu))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run {}: {error}", toolchain.clang.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(tu.source.as_bytes())
            .map_err(|error| format!("cannot write the TU: {error}"))?;
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(Outcome {
                    ok: false,
                    stderr: "timeout after 60 s".to_string(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => return Err(format!("waiting for clang failed: {error}")),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("reading clang output failed: {error}"))?;
    Ok(Outcome {
        ok: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn keep_tu(tu: &Tu) {
    let Some(directory) = std::env::var_os("HDRCHECK_KEEP") else {
        return;
    };
    let directory = PathBuf::from(directory);
    if std::fs::create_dir_all(&directory).is_err() {
        return;
    }
    let extension = if tu.lang.is_cxx() { "cc" } else { "c" };
    let name = format!(
        "{}-{}-{}-{:016x}.{extension}",
        tu.arch,
        tu.lang.name(),
        tu.vis.name(),
        fnv1a(tu.source.as_bytes())
    );
    let _ = std::fs::write(directory.join(name), &tu.source);
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Compile many TUs with a fixed number of worker threads.
///
/// Results are returned in the input order, which makes every caller's
/// output deterministic.
pub fn run_matrix(toolchain: &Toolchain, tus: Vec<Tu>, jobs: usize) -> Vec<(Tu, Outcome)> {
    let jobs = jobs.max(1).min(tus.len().max(1));
    let next = AtomicUsize::new(0);
    let total = tus.len();
    let mut indexed: Vec<(usize, Outcome)> = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..jobs {
            handles.push(scope.spawn(|| {
                let mut local = Vec::new();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= total {
                        break;
                    }
                    local.push((index, compile(toolchain, &tus[index])));
                }
                local
            }));
        }
        let mut all = Vec::with_capacity(total);
        for handle in handles {
            match handle.join() {
                Ok(local) => all.extend(local),
                Err(_) => all.push((
                    usize::MAX,
                    Outcome {
                        ok: false,
                        stderr: "worker thread panicked".to_string(),
                    },
                )),
            }
        }
        all
    });
    indexed.sort_by_key(|(index, _)| *index);
    let mut outcomes: Vec<Option<Outcome>> = (0..total).map(|_| None).collect();
    for (index, outcome) in indexed {
        if let Some(slot) = outcomes.get_mut(index) {
            *slot = Some(outcome);
        }
    }
    tus.into_iter()
        .zip(outcomes)
        .map(|(tu, outcome)| {
            let outcome = outcome.unwrap_or(Outcome {
                ok: false,
                stderr: "missing result".to_string(),
            });
            (tu, outcome)
        })
        .collect()
}

/// Run clang with full control over the include directories. `ours` adds the
/// ravenlibc `-isystem` pair; `freestanding` adds `-ffreestanding`.
pub fn compile_custom(
    toolchain: &Toolchain,
    arch: Arch,
    lang: Lang,
    ours: bool,
    freestanding: bool,
    extra: &[&str],
    source: &str,
) -> Outcome {
    let mut args = vec![
        target(toolchain, arch),
        "-fsyntax-only".to_string(),
        "-Werror".to_string(),
        "-pedantic".to_string(),
        "-Wall".to_string(),
        "-Wextra".to_string(),
    ];
    if ours {
        args.push("-nostdinc".to_string());
        args.push("-isystem".to_string());
        args.push(toolchain.resource_include.display().to_string());
        args.push("-isystem".to_string());
        args.push(arch.include_dir().display().to_string());
        args.push("-isystem".to_string());
        args.push(
            repo_root()
                .join("libc/ravenlibc/include")
                .display()
                .to_string(),
        );
    }
    if freestanding {
        args.push("-ffreestanding".to_string());
    }
    args.extend(extra.iter().map(|value| (*value).to_string()));
    if lang.is_cxx() {
        args.push("-x".to_string());
        args.push("c++".to_string());
        args.push(format!("-std={}", lang.std_flag(toolchain)));
    } else {
        args.push("-x".to_string());
        args.push("c".to_string());
        args.push(format!("-std={}", lang.std_flag(toolchain)));
    }
    args.push("-".to_string());
    run_child(toolchain, &args, source)
}

fn run_child(toolchain: &Toolchain, args: &[String], source: &str) -> Outcome {
    let mut child = match Command::new(&toolchain.clang)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Outcome {
                ok: false,
                stderr: format!("cannot run clang: {error}"),
            };
        }
    };
    if let Some(mut stdin) = child.stdin.take()
        && stdin.write_all(source.as_bytes()).is_err()
    {
        return Outcome {
            ok: false,
            stderr: "cannot write the TU".to_string(),
        };
    }
    match child.wait_with_output() {
        Ok(output) => Outcome {
            ok: output.status.success(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(error) => Outcome {
            ok: false,
            stderr: format!("waiting for clang failed: {error}"),
        },
    }
}

/// Compile a source with extra `-D` arguments, bypassing the `Tu` vector.
pub fn compile_with(
    toolchain: &Toolchain,
    arch: Arch,
    lang: Lang,
    defines: &[&str],
    source: &str,
) -> Outcome {
    let mut args = clang_args(
        toolchain,
        &Tu {
            arch,
            lang,
            vis: Vis::Default,
            source: String::new(),
        },
    );
    // Insert before the final `-`.
    let position = args.len().saturating_sub(1);
    for define in defines {
        args.insert(position, (*define).to_string());
    }
    let mut child = match Command::new(&toolchain.clang)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Outcome {
                ok: false,
                stderr: format!("cannot run clang: {error}"),
            };
        }
    };
    if let Some(mut stdin) = child.stdin.take()
        && stdin.write_all(source.as_bytes()).is_err()
    {
        return Outcome {
            ok: false,
            stderr: "cannot write the TU".to_string(),
        };
    }
    match child.wait_with_output() {
        Ok(output) => Outcome {
            ok: output.status.success(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(error) => Outcome {
            ok: false,
            stderr: format!("waiting for clang failed: {error}"),
        },
    }
}

/// Run `clang -E -dM` and parse `#define NAME VALUE` pairs.
pub fn macro_dump(
    toolchain: &Toolchain,
    arch: Arch,
    lang: Lang,
    source: &str,
    defines: &[&str],
) -> Result<Vec<(String, String)>, String> {
    let mut args = clang_args(
        toolchain,
        &Tu {
            arch,
            lang,
            vis: Vis::Default,
            source: String::new(),
        },
    );
    let position = args.len().saturating_sub(1);
    for define in defines {
        args.insert(position, (*define).to_string());
    }
    // Replace -fsyntax-only with -E -dM.
    args.retain(|arg| arg != "-fsyntax-only");
    args.insert(args.len() - 1, "-E".to_string());
    args.insert(args.len() - 1, "-dM".to_string());
    let mut child = Command::new(&toolchain.clang)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run clang: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(source.as_bytes())
            .map_err(|error| format!("cannot write the TU: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("waiting for clang failed: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut pairs = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let mut parts = rest.splitn(2, char::is_whitespace);
        let Some(name) = parts.next() else {
            continue;
        };
        if name.contains('(') {
            continue;
        }
        pairs.push((
            name.to_string(),
            parts.next().unwrap_or("").trim().to_string(),
        ));
    }
    pairs.sort();
    Ok(pairs)
}

#[cfg(test)]
#[path = "clang_ut.rs"]
mod tests;
