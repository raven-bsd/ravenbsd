// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cargo xtask build uutils`: build the selected uutils coreutils for the Raven std target.
//!
//! The vendored workspace lives in `userland/uutils` and is its own cargo workspace. It is built
//! with the custom `ravenbsd` toolchain and the Raven std target, never the host target. The
//! selected set is the first K25a slice; adding a name needs a test and a man page.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::process::{CommandSpec, run as run_command};

/// First selected set: Unix name and uutils crate. The binary name is the Unix name.
pub(crate) const SELECTED: &[(&str, &str)] = &[
    ("ls", "uu_ls"),
    ("cat", "uu_cat"),
    ("cp", "uu_cp"),
    ("mv", "uu_mv"),
    ("rm", "uu_rm"),
    ("ln", "uu_ln"),
    ("mkdir", "uu_mkdir"),
    ("rmdir", "uu_rmdir"),
    ("touch", "uu_touch"),
    ("stat", "uu_stat"),
    ("df", "uu_df"),
    ("du", "uu_du"),
    ("echo", "uu_echo"),
    ("pwd", "uu_pwd"),
    ("basename", "uu_basename"),
    ("dirname", "uu_dirname"),
    ("true", "uu_true"),
    ("false", "uu_false"),
    ("sleep", "uu_sleep"),
    ("date", "uu_date"),
    ("head", "uu_head"),
    ("tail", "uu_tail"),
    ("wc", "uu_wc"),
    ("sort", "uu_sort"),
    ("cut", "uu_cut"),
    ("tr", "uu_tr"),
    ("env", "uu_env"),
    ("id", "uu_id"),
    ("chmod", "uu_chmod"),
    ("chown", "uu_chown"),
    ("tty", "uu_tty"),
];

const USAGE: &str = "\
usage:
  cargo xtask build uutils [--arch x86_64|aarch64|riscv64] [--release|--debug] [--list]";

/// Parsed options for one build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Options {
    /// Architecture name.
    pub(crate) arch: String,
    /// Release profile. Release is the default: the 64 KiB user stack cannot hold the
    /// stack frames a debug build of `ls`/`cat` generates.
    pub(crate) release: bool,
    /// Print the selected table and do nothing.
    pub(crate) list: bool,
}

/// Parse the forwarded `build uutils` arguments.
pub(crate) fn parse_options(args: &[String]) -> Result<Options> {
    let mut options = Options {
        arch: "x86_64".to_string(),
        release: true,
        list: false,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--arch" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    bail!("--arch needs a value\n{USAGE}");
                };
                if arch_triple(value).is_none() {
                    bail!("unknown architecture `{value}`\n{USAGE}");
                }
                options.arch = value.clone();
            }
            "--release" => options.release = true,
            "--debug" => options.release = false,
            "--list" => options.list = true,
            other => bail!("unknown uutils option `{other}`\n{USAGE}"),
        }
        index += 1;
    }
    Ok(options)
}

/// The Raven std target triple.
#[must_use]
pub(crate) fn arch_triple(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("x86_64-unknown-ravenbsd"),
        "aarch64" => Some("aarch64-unknown-ravenbsd"),
        "riscv64" => Some("riscv64gc-unknown-ravenbsd"),
        _ => None,
    }
}

/// The package selected for a Unix name (test-side table check).
#[cfg(test)]
#[must_use]
pub(crate) fn package_for(name: &str) -> Option<&'static str> {
    SELECTED
        .iter()
        .find(|(unix_name, _)| *unix_name == name)
        .map(|(_, package)| *package)
}

/// The cargo argument vector.
#[must_use]
pub(crate) fn command_args(root: &Path, options: &Options) -> Vec<String> {
    let triple = arch_triple(&options.arch).unwrap_or("x86_64-unknown-ravenbsd");
    let target_dir = root
        .join("build/cargo")
        .join(format!("uutils-{}", options.arch));
    let mut args = vec![
        "+ravenbsd".to_string(),
        "build".to_string(),
        "--locked".to_string(),
        "--manifest-path".to_string(),
        root.join("userland/uutils/Cargo.toml")
            .display()
            .to_string(),
        "--target".to_string(),
        triple.to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
    ];
    if options.release {
        args.push("--release".to_string());
    }
    for (_, package) in SELECTED {
        args.push("-p".to_string());
        args.push((*package).to_string());
    }
    args
}

/// The build output directory for one binary.
#[must_use]
pub(crate) fn artifact_path(root: &Path, options: &Options, name: &str) -> PathBuf {
    let triple = arch_triple(&options.arch).unwrap_or("x86_64-unknown-ravenbsd");
    let profile = if options.release { "release" } else { "debug" };
    root.join("build/cargo")
        .join(format!("uutils-{}", options.arch))
        .join(triple)
        .join(profile)
        .join(name)
}

/// The installed in-image staging path.
#[must_use]
pub(crate) fn install_path(root: &Path, options: &Options, name: &str) -> PathBuf {
    root.join("build")
        .join(&options.arch)
        .join("uutils")
        .join(name)
}

/// Build and stage every selected utility.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<u8> {
    let options = parse_options(args)?;
    if options.list {
        for (name, package) in SELECTED {
            println!("{name}\t{package}");
        }
        return Ok(0);
    }
    let triple = arch_triple(&options.arch).unwrap_or("x86_64-unknown-ravenbsd");
    println!(
        "==> uutils {} ({}{})",
        options.arch,
        triple,
        if options.release { ", release" } else { "" }
    );
    let head = crate::repo::head_info(root)?;
    // K25a: the Raven std target does not add `-lc` itself; the static ralibc archive
    // and the getrandom custom-backend cfg are supplied here, next to the build.
    let stage = root.join("build").join(&options.arch);
    let libc = stage.join("libc.a");
    let crt = stage.join("crt1.o");
    let crti = stage.join("crti.o");
    let crtn = stage.join("crtn.o");
    let std_linker = root
        .join("userland/uutils")
        .join(format!("{}.ld", options.arch));
    let linker = if std_linker.is_file() {
        std_linker
    } else {
        root.join("userland/linker")
            .join(format!("{}.ld", options.arch))
    };
    let code_model = if options.arch == "x86_64" {
        " -C code-model=small"
    } else {
        ""
    };
    let rustflags = format!(
        "--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static{} \
         -C link-arg={} -C link-arg={} -C link-arg={} -C link-arg={} -C link-arg=-T{}",
        code_model,
        crt.display(),
        crti.display(),
        libc.display(),
        crtn.display(),
        linker.display()
    );
    let spec = CommandSpec::new("cargo", command_args(root, &options), root)
        .env("RUSTFLAGS", rustflags)
        .env("SOURCE_DATE_EPOCH", head.epoch.to_string())
        .env(
            "RAVEN_BUILD_ID",
            crate::build::build_id_from(head.epoch, &head.short, head.dirty),
        );
    let code = run_command(&spec)?;
    if code != 0 {
        eprintln!("xtask: uutils target build failed with exit code {code}");
        return Ok(u8::try_from(code).unwrap_or(1));
    }
    let staging = root.join("build").join(&options.arch).join("uutils");
    fs::create_dir_all(&staging).with_context(|| format!("cannot create {}", staging.display()))?;
    for (name, _) in SELECTED {
        let source = artifact_path(root, &options, name);
        let destination = install_path(root, &options, name);
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "cannot copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
        println!("    {}", destination.display());
    }
    Ok(0)
}

#[cfg(test)]
#[path = "uutils_ut.rs"]
mod tests;
