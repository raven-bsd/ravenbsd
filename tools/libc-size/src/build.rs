// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build steps of the probe matrix (`C69` 4.4): subprocesses with an argument vector,
//! never a shell.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};

use crate::tools::Tools;
use crate::{Arch, Lang, Linkage, Set};

/// One child process, fully described.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    program: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
}

impl CommandSpec {
    /// Build a spec.
    #[must_use]
    pub fn new(program: &Path, args: Vec<String>, cwd: PathBuf) -> Self {
        Self {
            program: program.to_path_buf(),
            args,
            cwd,
        }
    }

    /// The program.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// The argument vector.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// The working directory.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// A shell-free rendering for logs (the path is not a result file).
    #[must_use]
    pub fn display_line(&self) -> String {
        let mut out = self.program.display().to_string();
        for arg in &self.args {
            out.push(' ');
            if arg.contains(' ') {
                out.push('"');
                out.push_str(arg);
                out.push('"');
            } else {
                out.push_str(arg);
            }
        }
        out
    }

    /// Run the process and capture both streams.
    ///
    /// # Errors
    /// The process cannot be spawned.
    pub fn run(&self) -> Result<Output> {
        Command::new(&self.program)
            .args(&self.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .output()
            .with_context(|| format!("cannot run {}", self.program.display()))
    }

    /// Run and fail on a non-zero exit with the last `error:` line.
    ///
    /// # Errors
    /// A non-zero exit or a spawn error.
    pub fn run_checked(&self, what: &str) -> Result<()> {
        let output = self.run()?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{what}: {}", compact_error(&stderr));
    }
}

/// Pick the most useful line of a tool failure (the last `error:` line, else the last line).
#[must_use]
pub fn compact_error(stderr: &str) -> String {
    let mut found = String::new();
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.contains("undefined symbol") {
            found = trimmed.to_string();
            break;
        }
        if found.is_empty()
            && (trimmed.contains("error:") || trimmed.starts_with("error["))
            && !trimmed.contains("aborting due to")
            && !trimmed.contains("linking with")
        {
            found = trimmed.to_string();
        }
    }
    if !found.is_empty() {
        return found;
    }
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("tool failed")
        .to_string()
}

/// The outputs of one built probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifacts {
    /// Unstripped executable.
    pub binary: PathBuf,
    /// Stripped executable.
    pub stripped: PathBuf,
    /// The lld map.
    pub map: PathBuf,
}

/// Build the C61 START-FINDING support object (`__progname*`, `__stack_chk_guard`,
/// `__cxa_atexit`, `__raven_init_main_thread`).
///
/// # Errors
/// A rustc or I/O failure.
pub fn compile_start_support(
    tools: &Tools,
    root: &Path,
    arch: Arch,
    out_dir: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create {}", out_dir.display()))?;
    let source = crate::probe::start_support_path(root);
    let object = out_dir.join("start_support.o");
    let spec = CommandSpec::new(
        &tools.rustc,
        vec![
            "--edition".to_string(),
            "2024".to_string(),
            "--crate-type".to_string(),
            "lib".to_string(),
            "--emit=obj".to_string(),
            "--target".to_string(),
            arch.triple().to_string(),
            "-C".to_string(),
            "panic=abort".to_string(),
            "-C".to_string(),
            "opt-level=z".to_string(),
            "-C".to_string(),
            "strip=none".to_string(),
            "-o".to_string(),
            object.display().to_string(),
            source.display().to_string(),
        ],
        root.to_path_buf(),
    );
    spec.run_checked("start support")?;
    Ok(object)
}

/// Build one probe row.
///
/// # Errors
/// A compile/link failure; the caller records it as a `skipped: <reason>` row.
#[allow(clippy::too_many_arguments)]
pub fn build_probe(
    tools: &Tools,
    root: &Path,
    arch: Arch,
    set: Set,
    linkage: Linkage,
    lang: Lang,
    id: &str,
    libc_dir: &Path,
    out_dir: &Path,
    p9_symbols: &[String],
) -> Result<Artifacts> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create {}", out_dir.display()))?;
    let support = compile_start_support(tools, root, arch, out_dir)?;
    let _ = &support;
    let spec = match (lang, linkage) {
        (Lang::Rust, Linkage::Static) => crate::probe::rust_static_command(
            &tools.rustc,
            root,
            arch,
            set,
            id,
            libc_dir,
            out_dir,
            p9_symbols,
        ),
        (Lang::Rust, Linkage::Dynamic) => crate::probe::rust_dynamic_command(
            &tools.rustc,
            root,
            arch,
            id,
            libc_dir,
            out_dir,
            p9_symbols,
        ),
        (Lang::C, Linkage::Static) => {
            let compile = crate::probe::c_compile_command(
                tools
                    .clang
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("no C toolchain"))?,
                root,
                arch,
                set,
                id,
                out_dir,
            );
            compile.run_checked("clang")?;
            crate::probe::c_link_command(&tools.lld, root, arch, id, libc_dir, out_dir)
        }
        (Lang::C, Linkage::Dynamic) => {
            let compile = crate::probe::c_compile_command(
                tools
                    .clang
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("no C toolchain"))?,
                root,
                arch,
                set,
                id,
                out_dir,
            );
            compile.run_checked("clang")?;
            crate::probe::c_link_dynamic_command(&tools.lld, root, arch, id, libc_dir, out_dir)
        }
    };
    spec.run_checked("link")?;
    let binary = out_dir.join(format!("{}.elf", id.to_ascii_lowercase()));
    let stripped = out_dir.join(format!("{}.stripped", id.to_ascii_lowercase()));
    let strip = CommandSpec::new(
        &tools.strip,
        vec![
            "--strip-all".to_string(),
            "-o".to_string(),
            stripped.display().to_string(),
            binary.display().to_string(),
        ],
        root.to_path_buf(),
    );
    strip.run_checked("llvm-strip")?;
    Ok(Artifacts {
        binary,
        stripped,
        map: out_dir.join(format!("{}.map", id.to_ascii_lowercase())),
    })
}

/// Whether the static products exist under `libc_dir`.
#[must_use]
pub fn static_products(libc_dir: &Path) -> bool {
    libc_dir.join("libc.a").is_file() && libc_dir.join("crt1.o").is_file()
}

/// The exact path of the dynamic product (`C62` has not landed on this branch).
#[must_use]
pub fn dynamic_product(libc_dir: &Path) -> PathBuf {
    libc_dir.join("libc.so.1")
}

/// A `cargo xtask libc-pack` command (the products of `C60`).
#[must_use]
pub fn libc_pack_command(cargo: &Path, root: &Path, arch: Arch, set: Set) -> CommandSpec {
    CommandSpec::new(
        cargo,
        vec![
            "xtask".to_string(),
            "libc-pack".to_string(),
            "--arch".to_string(),
            arch.short().to_string(),
            "--set".to_string(),
            set.as_str().to_string(),
        ],
        root.to_path_buf(),
    )
}

/// Build `libc.a` and the crt objects for one `(arch, set)`.
///
/// # Errors
/// A cargo failure.
pub fn ensure_libc(tools: &Tools, root: &Path, arch: Arch, set: Set) -> Result<()> {
    libc_pack_command(&tools.cargo, root, arch, set).run_checked("libc-pack")
}

#[cfg(test)]
#[path = "build_ut.rs"]
mod tests;
