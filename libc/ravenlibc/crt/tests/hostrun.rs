// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! H1-H4 of C61 section 7: the produced start objects and the real `__raven_libc_start` run as
//! native Linux x86-64 programs. The test is host-only (`SKIP: host` elsewhere); the objects
//! are `x86_64-unknown-none` machine code, the stack at entry is the SysV stack of Linux
//! (`CONTRACTS.md` C8).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGET: &str = "x86_64-unknown-none";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("crate lives under libc/ravenlibc/crt")
        .to_path_buf()
}

fn work_dir() -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("c61-hostrun");
    std::fs::create_dir_all(&dir).expect("cannot create the host-run directory");
    dir
}

fn rustc() -> String {
    std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string())
}

fn lld() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("LD_LLD")
        && !value.is_empty()
    {
        return Some(PathBuf::from(value));
    }
    let output = Command::new(rustc())
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = Command::new(rustc()).args(["-vV"]).output().ok()?;
    let host = String::from_utf8_lossy(&version.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))?;
    let candidate = Path::new(&sysroot)
        .join("lib/rustlib")
        .join(host)
        .join("bin/gcc-ld/ld.lld");
    if candidate.is_file() {
        return Some(candidate);
    }
    Some(PathBuf::from("ld.lld"))
}

fn compile(source: &Path, output: &Path) -> Result<(), String> {
    let status = Command::new(rustc())
        .args(["--edition", "2024", "--crate-type", "lib", "--emit=obj"])
        .args(["--target", TARGET])
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
        .arg(source)
        .status()
        .map_err(|error| format!("cannot run rustc: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("rustc failed for {}", source.display()))
    }
}

/// Build one start object through the E2 fallback (`crt/objs/<name>.rs`).
fn start_object(name: &str, output: &Path) -> Result<(), String> {
    let source = repo_root()
        .join("libc/ravenlibc/crt/objs")
        .join(format!("{name}.rs"));
    compile(&source, output)
}

fn fixture(name: &str, output: &Path) -> Result<(), String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.rs"));
    compile(&source, output)
}

fn link(linker: &Path, args: &[&str], objects: &[&Path], output: &Path) -> Result<(), String> {
    let mut command = Command::new(linker);
    command.args(args);
    for object in objects {
        command.arg(object);
    }
    let result = command
        .arg("-o")
        .arg(output)
        .output()
        .map_err(|error| format!("cannot run ld.lld: {error}"))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ld.lld failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
    }
}

fn run(binary: &Path, arg: &str, env: Option<(&str, &str)>) -> Output {
    let mut command = Command::new(binary);
    command.arg(arg).env_clear();
    if let Some((key, value)) = env {
        command.env(key, value);
    }
    command.output().expect("cannot run the linked program")
}

fn require_host() -> bool {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        true
    } else {
        println!("SKIP: host");
        false
    }
}

#[test]
fn h1_static_pie_with_relr() {
    if !require_host() {
        return;
    }
    let Some(linker) = lld() else {
        println!("SKIP: ld.lld not found");
        return;
    };
    let dir = work_dir();
    let start = dir.join("h1-rcrt1.o");
    let program = dir.join("h1-pie");
    start_object("rcrt1", &start).expect("rcrt1 object");
    let body = dir.join("h1-body.o");
    fixture("h1_static_pie", &body).expect("h1 fixture");
    link(
        &linker,
        &[
            "-pie",
            "--no-dynamic-linker",
            "-z",
            "pack-relative-relocs",
            "-e",
            "_start",
        ],
        &[&start, &body],
        &program,
    )
    .expect("h1 link");
    let output = run(&program, "", None);
    assert!(
        output.status.success(),
        "H1 RELR exit {:?}",
        output.status.code()
    );
}

#[test]
fn h1_static_pie_with_rela_only() {
    if !require_host() {
        return;
    }
    let Some(linker) = lld() else {
        println!("SKIP: ld.lld not found");
        return;
    };
    let dir = work_dir();
    let start = dir.join("h1b-rcrt1.o");
    let program = dir.join("h1b-pie");
    start_object("rcrt1", &start).expect("rcrt1 object");
    let body = dir.join("h1b-body.o");
    fixture("h1_static_pie", &body).expect("h1 fixture");
    link(
        &linker,
        &["-pie", "--no-dynamic-linker", "-e", "_start"],
        &[&start, &body],
        &program,
    )
    .expect("h1b link");
    let output = run(&program, "", None);
    assert!(
        output.status.success(),
        "H1 RELA exit {:?}",
        output.status.code()
    );
}

#[test]
fn h1_mutation_without_selfreloc_fails() {
    if !require_host() {
        return;
    }
    let Some(linker) = lld() else {
        println!("SKIP: ld.lld not found");
        return;
    };
    let dir = work_dir().join("mutated");
    std::fs::create_dir_all(&dir).expect("cannot create the mutation directory");
    for name in ["note.rs", "selfreloc.rs", "selfreloc_consts.rs", "rcrt1.rs"] {
        std::fs::copy(
            repo_root().join("libc/ravenlibc/crt/src").join(name),
            dir.join(name),
        )
        .expect("cannot copy a crt source");
    }
    let source = dir.join("rcrt1.rs");
    let text = std::fs::read_to_string(&source).expect("cannot read rcrt1.rs");
    let mutated = text.replace(
        "let _base = unsafe { crate::selfreloc::relocate_self(sp, dynv) };",
        "let _base = 0usize; // mutation: the self-relocation is deliberately absent",
    );
    assert_ne!(text, mutated, "the mutation point changed");
    std::fs::write(&source, mutated).expect("cannot write the mutated rcrt1.rs");
    let root = dir.join("lib.rs");
    std::fs::write(
        &root,
        "#![no_std]\n#[macro_use]\n#[path = \"note.rs\"]\nmod note;\n#[path = \"selfreloc_consts.rs\"]\nmod selfreloc_consts;\n#[path = \"selfreloc.rs\"]\nmod selfreloc;\n#[path = \"rcrt1.rs\"]\nmod rcrt1;\n",
    )
    .expect("cannot write the mutation root");
    let start = dir.join("mutated-rcrt1.o");
    compile(&root, &start).expect("mutated rcrt1 object");
    let body = dir.join("body.o");
    fixture("h1_static_pie", &body).expect("h1 fixture");
    let program = dir.join("mutated-pie");
    link(
        &linker,
        &[
            "-pie",
            "--no-dynamic-linker",
            "-z",
            "pack-relative-relocs",
            "-e",
            "_start",
        ],
        &[&start, &body],
        &program,
    )
    .expect("mutation link");
    let output = run(&program, "", None);
    assert!(
        !output.status.success(),
        "the mutation proof passed: selfreloc is not exercised"
    );
}

#[test]
fn h2_static_non_pie() {
    if !require_host() {
        return;
    }
    let Some(linker) = lld() else {
        println!("SKIP: ld.lld not found");
        return;
    };
    let dir = work_dir();
    let start = dir.join("h2-crt1.o");
    let program = dir.join("h2-static");
    start_object("crt1", &start).expect("crt1 object");
    let body = dir.join("h2-body.o");
    fixture("h2_static", &body).expect("h2 fixture");
    link(
        &linker,
        &["-static", "-e", "_start"],
        &[&start, &body],
        &program,
    )
    .expect("h2 link");
    let output = run(&program, "hello", Some(("H2ENV", "1")));
    assert!(
        output.status.success(),
        "H2 exit {:?}",
        output.status.code()
    );
}

#[test]
fn h3_scrt1_register_convention() {
    if !require_host() {
        return;
    }
    let Some(linker) = lld() else {
        println!("SKIP: ld.lld not found");
        return;
    };
    let dir = work_dir();
    let start = dir.join("h3-scrt1.o");
    let program = dir.join("h3-dynamic-start");
    start_object("scrt1", &start).expect("Scrt1 object");
    let body = dir.join("h3-body.o");
    fixture("h3_scrt1", &body).expect("h3 fixture");
    link(
        &linker,
        &["-static", "-e", "stub_entry"],
        &[&start, &body],
        &program,
    )
    .expect("h3 link");
    let output = run(&program, "", None);
    assert!(
        output.status.success(),
        "H3 exit {:?}",
        output.status.code()
    );
}

#[test]
fn h4_real_start_with_libc_archive() {
    if !require_host() {
        return;
    }
    let Some(linker) = lld() else {
        println!("SKIP: ld.lld not found");
        return;
    };
    let dir = work_dir();
    let archive = build_shell_archive(&dir).expect("libc.a");
    let start = dir.join("h4-crt1.o");
    let program = dir.join("h4-real-start");
    start_object("crt1", &start).expect("crt1 object");
    let body = dir.join("h4-body.o");
    fixture("h4_real_start", &body).expect("h4 fixture");
    let mut command = Command::new(&linker);
    command
        .args(["-static", "--gc-sections", "-e", "_start"])
        .arg(&start)
        .arg(&body)
        .arg(&archive)
        .arg("-o")
        .arg(&program);
    let result = command.output().expect("cannot run ld.lld");
    assert!(
        result.status.success(),
        "H4 link failed: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    let output = run(&program, "hello", Some(("H4ENV", "1")));
    assert!(
        output.status.success(),
        "H4 exit {:?}",
        output.status.code()
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("H4 OK"),
        "H4 did not print its marker"
    );
    check_start_budget(&program);
}

/// `donor-map/16` 3.A.9.1: the start path is at most 250 instructions before `main`.
fn check_start_budget(program: &Path) {
    let Ok(disassembly) = Command::new("objdump")
        .args(["-d", "--no-show-raw-insn"])
        .arg(program)
        .output()
    else {
        println!("SKIP: objdump not found for the start budget");
        return;
    };
    if !disassembly.status.success() {
        println!("SKIP: objdump failed for the start budget");
        return;
    }
    let text = String::from_utf8_lossy(&disassembly.stdout);
    let mut total = 0usize;
    for symbol in [
        "__raven_libc_start>:",
        "10raven_libc5start6stage2>:",
        "10raven_libc5start22run_constructor_arrays>:",
    ] {
        let mut inside = false;
        let mut count = 0usize;
        for line in text.lines() {
            if line.trim_end().ends_with(symbol) {
                inside = true;
                continue;
            }
            if inside {
                if line.trim().is_empty() {
                    break;
                }
                if line
                    .trim_start()
                    .starts_with(|character: char| character.is_ascii_hexdigit())
                    && line.contains(':')
                {
                    count += 1;
                }
            }
        }
        total += count;
    }
    assert!(
        total > 0 && total <= 250,
        "the start path has {total} instructions (budget 250)"
    );
}

/// Build the shell `libc.a` for the host-run link.
fn build_shell_archive(dir: &Path) -> Result<PathBuf, String> {
    let target_dir = dir.join("libc-a");
    let status = Command::new("cargo")
        .current_dir(repo_root())
        .args([
            "rustc",
            "-p",
            "raven-libc",
            "--crate-type",
            "staticlib",
            "--profile",
            "libc-a",
            "--target",
            TARGET,
            "--target-dir",
        ])
        .arg(&target_dir)
        .args(["--features", "server"])
        .env("RUSTFLAGS", "-C relocation-model=pic -C code-model=small")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err("the shell staticlib build failed".to_string());
    }
    let archive = target_dir
        .join(TARGET)
        .join("libc-a")
        .join("libraven_libc.a");
    if archive.is_file() {
        Ok(archive)
    } else {
        Err(format!("{} not found", archive.display()))
    }
}
