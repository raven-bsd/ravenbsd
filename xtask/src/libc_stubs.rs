// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `libc-stubs`: generate the empty archive and linker-script stub libraries of ticket C66 5.D
//! (`17-crate-layout.md` 7.3, `14` 3.A.8 + delta D-5).
//!
//! The generator is deterministic and involves no compiler: `libNAME.a` is the 8-byte
//! `!<arch>\n` archive header and `libNAME.so` is the 17-byte text `INPUT(libc.so.1)\n`.
//! `--check` regenerates in memory and compares the directory byte for byte, including extra
//! `lib*.a`/`lib*.so` files that the table does not name.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The 20 stub names of `17` 7.3, sorted.
pub(crate) const EXPECTED: &[&str] = &[
    "bsd",
    "compat",
    "crypt",
    "dl",
    "execinfo",
    "gen",
    "intl",
    "m",
    "nsl",
    "posix4",
    "pthread",
    "resolv",
    "rpcsvc",
    "rt",
    "socket",
    "ssp",
    "ssp_nonshared",
    "thread",
    "util",
    "xnet",
];

/// The empty archive bytes (`!<arch>\n`).
pub(crate) const EMPTY_ARCHIVE: &[u8; 8] = b"!<arch>\n";
/// The linker-script text.
pub(crate) const SCRIPT: &[u8; 17] = b"INPUT(libc.so.1)\n";

/// The table file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stubs {
    /// One row per stub.
    pub(crate) stub: Vec<Stub>,
}

/// One stub row.
///
/// `content`, `alias_soname` and `note` are documentation for the table reader; they are kept
/// in the schema so an unknown or misspelt key is an error rather than silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct Stub {
    /// `-lNAME`.
    pub(crate) name: String,
    /// Ring of the names conceptually living in the library.
    pub(crate) ring: u8,
    /// Free-text description of the content.
    pub(crate) content: String,
    /// Loader alias (informational, no file is generated for it).
    pub(crate) alias_soname: String,
    /// Why the stub exists.
    pub(crate) note: String,
}

impl Stubs {
    /// Load and validate the table.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let table: Self =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        table.validate()?;
        Ok(table)
    }

    fn validate(&self) -> Result<()> {
        let mut names = BTreeSet::new();
        for row in &self.stub {
            if row.name.is_empty()
                || row.name.contains('/')
                || row.name.contains('\\')
                || row.name.contains('.')
            {
                bail!("invalid stub name `{}`", row.name);
            }
            if !names.insert(row.name.clone()) {
                bail!("duplicate stub name `{}`", row.name);
            }
            if !(1..=3).contains(&row.ring) {
                bail!("stub `{}` has ring {} (expected 1..=3)", row.name, row.ring);
            }
        }
        let sorted: Vec<&str> = names.iter().map(String::as_str).collect();
        if sorted != EXPECTED {
            bail!(
                "stub table names differ from the 20 of 17 section 7.3: {}",
                sorted.join(",")
            );
        }
        Ok(())
    }
}

/// One generated file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Output {
    /// Relative file name.
    pub(crate) name: String,
    /// Exact bytes.
    pub(crate) bytes: Vec<u8>,
}

/// The expected file set, in name order.
pub(crate) fn outputs(table: &Stubs) -> Vec<Output> {
    let mut rows: Vec<&Stub> = table.stub.iter().collect();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    let mut files = Vec::with_capacity(rows.len().saturating_mul(2));
    for row in rows {
        files.push(Output {
            name: format!("lib{}.a", row.name),
            bytes: EMPTY_ARCHIVE.to_vec(),
        });
        files.push(Output {
            name: format!("lib{}.so", row.name),
            bytes: SCRIPT.to_vec(),
        });
    }
    files
}

fn set_mode(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644))
            .with_context(|| format!("cannot chmod {}", path.display()))?;
    }
    Ok(())
}

/// Write the full set, creating `out`.
pub(crate) fn generate(table: &Stubs, out: &Path) -> Result<usize> {
    fs::create_dir_all(out).with_context(|| format!("cannot create {}", out.display()))?;
    let files = outputs(table);
    for file in &files {
        let path = out.join(&file.name);
        fs::write(&path, &file.bytes)
            .with_context(|| format!("cannot write {}", path.display()))?;
        set_mode(&path)?;
        println!("libc-stubs: wrote {}", path.display());
    }
    Ok(files.len())
}

/// Compare the directory with the expected set; returns the findings.
pub(crate) fn check(table: &Stubs, out: &Path) -> Result<Vec<String>> {
    let mut findings = Vec::new();
    if !out.is_dir() {
        return Ok(findings);
    }
    let files = outputs(table);
    for file in &files {
        let path = out.join(&file.name);
        match fs::read(&path) {
            Ok(bytes) => {
                if bytes != file.bytes {
                    findings.push(format!(
                        "{} is {} bytes, expected {}",
                        path.display(),
                        bytes.len(),
                        file.bytes.len()
                    ));
                }
            }
            Err(_) => findings.push(format!("{} is missing", path.display())),
        }
    }
    let expected: BTreeSet<String> = files.into_iter().map(|file| file.name).collect();
    let entries = fs::read_dir(out).with_context(|| format!("cannot read {}", out.display()))?;
    let mut extra: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_stub_name = (name.starts_with("lib") && name.ends_with(".a"))
            || (name.starts_with("lib") && name.ends_with(".so"));
        if is_stub_name && !expected.contains(&name) {
            extra.push(name);
        }
    }
    extra.sort();
    for name in extra {
        findings.push(format!("{} is not in stubs.toml", out.join(name).display()));
    }
    Ok(findings)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arch {
    X86_64,
    Aarch64,
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

    fn short(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }
}

/// Run the command.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let mut arch = Arch::X86_64;
    let mut out: Option<PathBuf> = None;
    let mut check_only = false;
    let mut index = 0usize;
    while index < args.len() {
        match args.get(index).map(String::as_str) {
            Some("--arch") => {
                index += 1;
                arch = Arch::parse(args.get(index).context("--arch needs a value")?.as_str())?;
            }
            Some("--out") => {
                index += 1;
                out = Some(PathBuf::from(
                    args.get(index).context("--out needs a value")?,
                ));
            }
            Some("--check") => check_only = true,
            Some(other) => bail!("unknown libc-stubs flag `{other}`"),
            None => break,
        }
        index += 1;
    }
    let table = Stubs::load(&root.join("libc/ravenlibc/stubs/stubs.toml"))?;
    let directory = match out {
        Some(path) if path.is_absolute() => path,
        Some(path) => root.join(path),
        None => root.join(format!("build/{}/stubs", arch.short())),
    };
    if check_only {
        let findings = check(&table, &directory)?;
        if findings.is_empty() {
            println!(
                "libc-stubs: check: ok ({} files in {})",
                outputs(&table).len(),
                directory.display()
            );
            Ok(0)
        } else {
            for finding in &findings {
                eprintln!("libc-stubs: {finding}");
            }
            Ok(1)
        }
    } else {
        let count = generate(&table, &directory)?;
        println!("libc-stubs: wrote {count} files");
        Ok(0)
    }
}

#[cfg(test)]
#[path = "libc_stubs_ut.rs"]
mod tests;
