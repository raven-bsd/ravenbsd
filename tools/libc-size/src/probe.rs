// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The probe table and the command vectors of the four link recipes (`C69` 4.3, 4.4).
//!
//! The vectors are pure data so the tests can assert them without spawning anything. The
//! static Rust line carries the four flags of `donor-map/16` section 2 item 6
//! (`relocation-model=static`, `--gc-sections`, `-no-pie`, and the `strip` handling of 4.4:
//! `strip=none` at link because the measurement needs the unstripped symbol list; the
//! protocol then strips with `llvm-strip --strip-all`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::build::CommandSpec;
use crate::{Arch, Lang, Set};

/// One probe of the fixed table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// Stable id (`P0` ... `P9`).
    pub id: &'static str,
    /// One-line purpose.
    pub title: &'static str,
    /// Symbols the probe calls; a missing one makes the row `skipped`.
    pub symbols: &'static [&'static str],
}

/// The probes `C69` measures. `P2m` is listed `n/a`: no `--printf=m` mechanism exists.
pub const PROBES: &[Probe] = &[
    Probe {
        id: "P0",
        title: "main returns 0 (start -> exit(0))",
        symbols: &[],
    },
    Probe {
        id: "P1",
        title: "write(1, \"hi\\n\", 3), no stdio",
        symbols: &["write"],
    },
    Probe {
        id: "P2",
        title: "printf(\"%d\\n\", argc)",
        symbols: &["printf"],
    },
    Probe {
        id: "P3",
        title: "printf(\"%f\\n\", argc * 1.5)",
        symbols: &["printf"],
    },
    Probe {
        id: "P4",
        title: "pthread_create + pthread_join",
        symbols: &["pthread_create", "pthread_join"],
    },
    Probe {
        id: "P5",
        title: "getaddrinfo(argv[1], \"80\", 0, &r)",
        symbols: &["getaddrinfo", "freeaddrinfo"],
    },
    Probe {
        id: "P6",
        title: "dlopen(argv[1], RTLD_NOW)",
        symbols: &["dlopen", "dlerror"],
    },
    Probe {
        id: "P7",
        title: "strftime(\"%c\") + gmtime",
        symbols: &["strftime", "gmtime"],
    },
    Probe {
        id: "P8",
        title: "qsort + bsearch + strtol + regcomp/regexec",
        symbols: &[
            "qsort", "bsearch", "strtol", "regcomp", "regexec", "regfree",
        ],
    },
    Probe {
        id: "P9",
        title: "full link: -u every name of abi/Symbols.list",
        symbols: &[],
    },
];

/// The probe ids, in order.
#[must_use]
pub fn probe_ids() -> Vec<&'static str> {
    PROBES.iter().map(|probe| probe.id).collect()
}

/// Find a probe by id.
#[must_use]
pub fn find(id: &str) -> Option<&'static Probe> {
    PROBES.iter().find(|probe| probe.id == id)
}

/// Parse `--probe P0,P1` (or `all`).
///
/// # Errors
/// An unknown id.
pub fn parse_selection(value: &str) -> Result<Vec<&'static str>> {
    if value == "all" {
        return Ok(probe_ids());
    }
    let mut out = Vec::new();
    for item in value.split(',').filter(|item| !item.is_empty()) {
        let Some(probe) = find(item) else {
            anyhow::bail!("unknown probe `{item}`");
        };
        if !out.contains(&probe.id) {
            out.push(probe.id);
        }
    }
    if out.is_empty() {
        anyhow::bail!("no probe selected");
    }
    out.sort_by_key(|id| crate::report::probe_number(id));
    Ok(out)
}

/// The source file of a probe (`C69` RD-new: the probes live under `tests/libc/size/probes/`).
#[must_use]
pub fn source_path(root: &Path, lang: Lang, id: &str) -> PathBuf {
    let directory = match lang {
        Lang::Rust => root.join("tests/libc/size/probes/rust"),
        Lang::C => root.join("tests/libc/size/probes/c"),
    };
    match lang {
        Lang::Rust => directory.join(format!("{}.rs", id.to_ascii_lowercase())),
        Lang::C => directory.join(format!("{}.c", id.to_ascii_lowercase())),
    }
}

/// The pinned `rustc` arguments of a static Rust probe.
///
/// `opt-level` is `z` for `tiny` and `s` for `server` (`R-26`).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn rust_static_command(
    rustc: &Path,
    root: &Path,
    arch: Arch,
    set: Set,
    id: &str,
    libc_dir: &Path,
    out_dir: &Path,
    p9_symbols: &[String],
) -> CommandSpec {
    let source = if id == "P9" {
        out_dir.join("p9_all.rs")
    } else {
        source_path(root, Lang::Rust, id)
    };
    let binary = out_dir.join(format!("{}.elf", id.to_ascii_lowercase()));
    let map = out_dir.join(format!("{}.map", id.to_ascii_lowercase()));
    let mut args = vec![
        "--edition".to_string(),
        "2024".to_string(),
        "--crate-type".to_string(),
        "bin".to_string(),
        "--target".to_string(),
        arch.triple().to_string(),
        "-C".to_string(),
        "panic=abort".to_string(),
        "-C".to_string(),
        format!("opt-level={}", opt_level(set)),
        "-C".to_string(),
        "strip=none".to_string(),
        "-C".to_string(),
        "relocation-model=static".to_string(),
        "-C".to_string(),
        "link-arg=--gc-sections".to_string(),
        "-C".to_string(),
        "link-arg=-no-pie".to_string(),
        "-C".to_string(),
        format!("link-arg=-Map={}", map.display()),
        "-C".to_string(),
        format!("link-arg={}", libc_dir.join("crt1.o").display()),
        "-C".to_string(),
        format!("link-arg=-L{}", libc_dir.display()),
        "-C".to_string(),
        format!("link-arg={}", libc_dir.join("libc.a").display()),
    ];
    if id == "P9" {
        for symbol in p9_symbols {
            args.push("-C".to_string());
            args.push("link-arg=-u".to_string());
            args.push("-C".to_string());
            args.push(format!("link-arg={symbol}"));
        }
    }
    args.push("-o".to_string());
    args.push(binary.display().to_string());
    args.push(source.display().to_string());
    CommandSpec::new(rustc, args, root.to_path_buf())
}

/// The pinned `rustc` arguments of a dynamic Rust probe (`PG-53` link flags).
#[must_use]
pub fn rust_dynamic_command(
    rustc: &Path,
    root: &Path,
    arch: Arch,
    id: &str,
    libc_dir: &Path,
    out_dir: &Path,
    p9_symbols: &[String],
) -> CommandSpec {
    let source = if id == "P9" {
        out_dir.join("p9_all.rs")
    } else {
        source_path(root, Lang::Rust, id)
    };
    let binary = out_dir.join(format!("{}.elf", id.to_ascii_lowercase()));
    let map = out_dir.join(format!("{}.map", id.to_ascii_lowercase()));
    let mut args = vec![
        "--edition".to_string(),
        "2024".to_string(),
        "--crate-type".to_string(),
        "bin".to_string(),
        "--target".to_string(),
        arch.triple().to_string(),
        "-C".to_string(),
        "panic=abort".to_string(),
        "-C".to_string(),
        "opt-level=s".to_string(),
        "-C".to_string(),
        "strip=none".to_string(),
        "-C".to_string(),
        "relocation-model=pic".to_string(),
        "-C".to_string(),
        "link-arg=--gc-sections".to_string(),
        "-C".to_string(),
        "link-arg=-pie".to_string(),
        "-C".to_string(),
        "link-arg=--dynamic-linker=/libexec/ld-elf.so.1".to_string(),
        "-C".to_string(),
        "link-arg=-z".to_string(),
        "-C".to_string(),
        "link-arg=max-page-size=4096".to_string(),
        "-C".to_string(),
        format!("link-arg=-Map={}", map.display()),
        "-C".to_string(),
        format!("link-arg={}", libc_dir.join("Scrt1.o").display()),
        "-C".to_string(),
        format!("link-arg=-L{}", libc_dir.display()),
        "-C".to_string(),
        format!("link-arg={}", libc_dir.join("libc.so.1").display()),
    ];
    if id == "P9" {
        for symbol in p9_symbols {
            args.push("-C".to_string());
            args.push("link-arg=-u".to_string());
            args.push("-C".to_string());
            args.push(format!("link-arg={symbol}"));
        }
    }
    args.push("-o".to_string());
    args.push(binary.display().to_string());
    args.push(source.display().to_string());
    CommandSpec::new(rustc, args, root.to_path_buf())
}

/// The clang compile step of a C probe.
#[must_use]
pub fn c_compile_command(
    clang: &Path,
    root: &Path,
    arch: Arch,
    set: Set,
    id: &str,
    out_dir: &Path,
) -> CommandSpec {
    let resource = clang_resource_dir(clang);
    let source = source_path(root, Lang::C, id);
    let object = out_dir.join(format!("{}.o", id.to_ascii_lowercase()));
    let include = root.join("libc/ravenlibc/include");
    let arch_include = root
        .join("libc/ravenlibc/arch")
        .join(arch.short())
        .join("include");
    let generated = root.join("libc/ravenlibc/abi/generated/include");
    let mut args = vec![
        // clang 14 spells the target flag `-target`; the `--target=` form is clang 15+.
        "-target".to_string(),
        arch.c_triple().to_string(),
    ];
    if arch == Arch::Riscv64 {
        // Match `riscv64gc-unknown-none-elf`: rv64gc with the hard-float lp64d psABI.
        args.push("-march=rv64gc".to_string());
        args.push("-mabi=lp64d".to_string());
    }
    args.extend([
        "-ffreestanding".to_string(),
        "-nostdlib".to_string(),
        "-nostdinc".to_string(),
        "-isystem".to_string(),
        resource.display().to_string(),
        "-isystem".to_string(),
        include.display().to_string(),
        "-isystem".to_string(),
        arch_include.display().to_string(),
        "-isystem".to_string(),
        generated.display().to_string(),
        format!("-O{}", opt_level(set)),
        "-ffunction-sections".to_string(),
        "-fdata-sections".to_string(),
        "-c".to_string(),
        source.display().to_string(),
        "-o".to_string(),
        object.display().to_string(),
    ]);
    CommandSpec::new(clang, args, root.to_path_buf())
}

/// The `ld.lld` link step of a C probe.
#[must_use]
pub fn c_link_command(
    lld: &Path,
    root: &Path,
    _arch: Arch,
    id: &str,
    libc_dir: &Path,
    out_dir: &Path,
) -> CommandSpec {
    let object = out_dir.join(format!("{}.o", id.to_ascii_lowercase()));
    let binary = out_dir.join(format!("{}.elf", id.to_ascii_lowercase()));
    let map = out_dir.join(format!("{}.map", id.to_ascii_lowercase()));
    let args = vec![
        "-static".to_string(),
        "--gc-sections".to_string(),
        format!("-Map={}", map.display()),
        "-o".to_string(),
        binary.display().to_string(),
        libc_dir.join("crt1.o").display().to_string(),
        libc_dir.join("crti.o").display().to_string(),
        object.display().to_string(),
        libc_dir.join("libc.a").display().to_string(),
        libc_dir.join("crtn.o").display().to_string(),
    ];
    CommandSpec::new(lld, args, root.to_path_buf())
}

/// The `ld.lld` link step of a dynamic C probe.
#[must_use]
pub fn c_link_dynamic_command(
    lld: &Path,
    root: &Path,
    _arch: Arch,
    id: &str,
    libc_dir: &Path,
    out_dir: &Path,
) -> CommandSpec {
    let object = out_dir.join(format!("{}.o", id.to_ascii_lowercase()));
    let binary = out_dir.join(format!("{}.elf", id.to_ascii_lowercase()));
    let map = out_dir.join(format!("{}.map", id.to_ascii_lowercase()));
    let args = vec![
        "-pie".to_string(),
        "--gc-sections".to_string(),
        "--dynamic-linker=/libexec/ld-elf.so.1".to_string(),
        "-z".to_string(),
        "max-page-size=4096".to_string(),
        format!("-Map={}", map.display()),
        "-o".to_string(),
        binary.display().to_string(),
        libc_dir.join("Scrt1.o").display().to_string(),
        libc_dir.join("crti.o").display().to_string(),
        object.display().to_string(),
        libc_dir.join("libc.so.1").display().to_string(),
        libc_dir.join("crtn.o").display().to_string(),
    ];
    CommandSpec::new(lld, args, root.to_path_buf())
}

fn opt_level(set: Set) -> &'static str {
    match set {
        Set::Tiny => "z",
        Set::Server => "s",
    }
}

fn clang_resource_dir(clang: &Path) -> PathBuf {
    let output = std::process::Command::new(clang)
        .arg("-print-resource-dir")
        .output();
    match output {
        Ok(output) if output.status.success() => {
            PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
        }
        _ => PathBuf::from("/usr/lib/clang"),
    }
}

/// The generated `p9_all.rs`: a `no_std` program whose `main` returns 0.
pub const P9_SOURCE: &str = "\
// SPDX-License-Identifier: BSD-3-Clause\n\
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\
// Generated by libc-size: the P9 full-link program (C69 4.3).\n\
#![no_std]\n\
#![no_main]\n\
use core::ffi::c_int;\n\
#[panic_handler]\n\
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }\n\
#[unsafe(no_mangle)]\n\
pub extern \"C\" fn main(_argc: c_int, _argv: *const *const u8, _envp: *const *const u8) -> c_int { 0 }\n";

/// Write `p9_all.rs` into the output directory.
///
/// # Errors
/// An I/O error.
pub fn write_p9_source(out_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create {}", out_dir.display()))?;
    let path = out_dir.join("p9_all.rs");
    std::fs::write(&path, P9_SOURCE).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
#[path = "probe_ut.rs"]
mod tests;
