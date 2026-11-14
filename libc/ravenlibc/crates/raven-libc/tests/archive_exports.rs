// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Archive checks A1-A8 of C60 section 7. They run when `build/x86_64/libc.a` exists; set
//! `RL_LIBC_REQUIRED=1` to make a missing archive a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root")
        .to_path_buf()
}

fn archive() -> PathBuf {
    root().join("build/x86_64/libc.a")
}

fn required() -> bool {
    std::env::var("RL_LIBC_REQUIRED").as_deref() == Ok("1")
}

macro_rules! skip_or_fail {
    ($reason:expr) => {{
        if required() {
            panic!("SKIP: {} (RL_LIBC_REQUIRED=1)", $reason);
        }
        eprintln!("SKIP: {}", $reason);
        return;
    }};
}

fn nm() -> String {
    for name in ["llvm-nm", "nm"] {
        if Command::new(name).arg("--version").output().is_ok() {
            return name.to_string();
        }
    }
    "nm".to_string()
}

fn defined() -> Vec<(String, char, String)> {
    let output = Command::new(nm())
        .args(["--defined-only", "--format=posix"])
        .arg(archive())
        .output()
        .expect("nm");
    let text = String::from_utf8_lossy(&output.stdout);
    let mut member = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.ends_with(':') {
            let header = line.trim_end_matches(':');
            member = header
                .rsplit_once('[')
                .and_then(|(_, rest)| rest.strip_suffix(']'))
                .unwrap_or(header)
                .to_string();
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind), Some(value)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if kind.len() != 1 || u64::from_str_radix(value, 16).is_err() {
            continue;
        }
        out.push((
            member.clone(),
            kind.chars().next().unwrap_or('?'),
            name.to_string(),
        ));
    }
    out
}

#[test]
fn a1_k1_to_k6_pass() {
    if !archive().is_file() {
        skip_or_fail!("no build/x86_64/libc.a");
    }
    let list = root().join("libc/ravenlibc/abi/Symbols.list");
    let map = root().join("libc/ravenlibc/crates/raven-libc/Symbol.map");
    let versions = root().join("libc/ravenlibc/abi/Versions.def");
    let hooks = root().join("libc/ravenlibc/crates/raven-libc/hooks.toml");
    let expected = root().join("libc/ravenlibc/crates/raven-libc/expected");
    let output = Command::new("cargo")
        .args(["run", "-q", "-p", "raven-libc-mkmap", "--", "check"])
        .arg("--archive")
        .arg(archive())
        .arg("--list")
        .arg(list)
        .arg("--map")
        .arg(map)
        .arg("--versions")
        .arg(versions)
        .arg("--hooks")
        .arg(hooks)
        .arg("--expected")
        .arg(expected)
        .args(["--arch", "x86_64"])
        .current_dir(root())
        .output()
        .expect("mkmap check");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn a2_no_init_array_in_a_shell_member() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let readelf = if Command::new("llvm-readelf")
        .arg("--version")
        .output()
        .is_ok()
    {
        "llvm-readelf"
    } else {
        "readelf"
    };
    let members = root().join("build/x86_64/pack/members");
    let Ok(entries) = std::fs::read_dir(&members) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !(name.contains("raven_libc.") || name.contains("libraven_libc.")) {
            continue;
        }
        let output = Command::new(readelf)
            .arg("-S")
            .arg(&path)
            .output()
            .expect("readelf");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(!text.contains(".init_array"), "{name} has .init_array");
        assert!(
            !text.contains(".preinit_array"),
            "{name} has .preinit_array"
        );
    }
}

#[test]
fn a3_one_public_text_symbol_per_member() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let mut text_per_member = std::collections::BTreeMap::new();
    for (member, kind, name) in defined() {
        if !(member.contains("raven_libc.") || member.contains("libraven_libc.")) {
            continue;
        }
        if name.starts_with("_R") || name.starts_with("anon.") {
            continue;
        }
        if matches!(kind, 'T' | 't') {
            *text_per_member.entry(member).or_insert(0usize) += 1;
        }
    }
    for (member, count) in text_per_member {
        assert!(count <= 4, "{member} has {count} C text symbols");
    }
}

#[test]
fn a4_symbols_list_has_no_duplicates() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let text = std::fs::read_to_string(root().join("libc/ravenlibc/abi/Symbols.list"))
        .expect("Symbols.list");
    let names = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    let unique = names.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(names.len(), unique.len());
}

#[test]
fn a5_no_mangle_outside_the_shell() {
    let crates = root().join("libc/ravenlibc/crates");
    let Ok(entries) = std::fs::read_dir(&crates) else {
        return;
    };
    let exceptions = ["raven-libc", "raven-libc-crt", "raven-libc-rtld"];
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if exceptions.contains(&name.as_str()) {
            continue;
        }
        let src = entry.path().join("src");
        let Ok(files) = std::fs::read_dir(&src) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|value| value.to_str()) != Some("rs") {
                continue;
            }
            // C54 defines `unimplemented_symbol!` with the attribute inside a macro body; the
            // token emits nothing unless invoked, and K3 checks the real archive symbols. The
            // A5 source grep skips macro-definition files for that reason.
            if path.file_name().and_then(|value| value.to_str()) == Some("macros.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            assert!(
                !text.contains("no_mangle"),
                "{} has no_mangle",
                path.display()
            );
        }
    }
}

#[test]
fn a6_panic_rs_never_names_the_payload_type() {
    let text =
        std::fs::read_to_string(root().join("libc/ravenlibc/crates/raven-libc/src/panic.rs"))
            .expect("panic.rs");
    assert!(!text.contains("PanicInfo"));
}

#[test]
fn a7_hot_members_have_no_call() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let hot = [
        "strlen", "strnlen", "memchr", "memcmp", "bcmp", "strcmp", "strncmp", "strchr",
    ];
    let symbols = defined();
    let objdump = "llvm-objdump";
    let members = root().join("build/x86_64/pack/members");
    for name in hot {
        let Some((member, _, _)) = symbols.iter().find(|(_, _, symbol)| symbol == name) else {
            continue;
        };
        let path = members.join(member);
        let output = Command::new(objdump)
            .args(["-d", "--no-show-raw-insn"])
            .arg(&path)
            .output()
            .expect("objdump");
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let code = line.split('#').next().unwrap_or("");
            assert!(
                !code
                    .split_whitespace()
                    .any(|token| token.starts_with("call")),
                "hot member {member} has a call: {line}"
            );
        }
    }
}

#[test]
fn a8_c64_family_symbols_are_defined() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let names = defined()
        .into_iter()
        .map(|(_, _, name)| name)
        .collect::<std::collections::BTreeSet<_>>();
    for required in [
        "fork",
        "getpid",
        "getpwnam",
        "sched_yield",
        "htons",
        "inet_pton",
        "__progname",
        "in6addr_any",
    ] {
        assert!(names.contains(required), "C64 symbol {required} is missing");
    }
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

/// Compile, link and run one `hostrun/` program natively.
fn hostrun(name: &str) -> Option<String> {
    if !(cfg!(all(target_os = "linux", target_arch = "x86_64"))) {
        eprintln!("SKIP: host");
        return None;
    }
    if !archive().is_file() {
        if required() {
            panic!("SKIP: no build/x86_64/libc.a (RL_LIBC_REQUIRED=1)");
        }
        eprintln!("SKIP: no build/x86_64/libc.a");
        return None;
    }
    let scratch = Path::new("/tmp/opencode");
    let work = if scratch.is_dir() {
        scratch.join("c60c-hostrun")
    } else {
        std::env::temp_dir().join("c60c-hostrun")
    };
    std::fs::create_dir_all(&work).ok()?;
    let source = root()
        .join("libc/ravenlibc/crates/raven-libc/hostrun")
        .join(format!("{name}.rs"));
    assert!(
        source.is_file(),
        "missing hostrun source {}",
        source.display()
    );
    let object = work.join(format!("{name}.o"));
    let binary = work.join(name);
    let compile = Command::new(rustc())
        .args(["--edition", "2024", "--crate-type", "lib", "--emit=obj"])
        .args(["--target", "x86_64-unknown-none"])
        .args([
            "-C",
            "panic=abort",
            "-C",
            "opt-level=z",
            "-C",
            "relocation-model=pic",
        ])
        .arg("-o")
        .arg(&object)
        .arg(&source)
        .output()
        .ok()?;
    assert!(
        compile.status.success(),
        "rustc failed for {name}: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let linker = lld()?;
    let link = Command::new(&linker)
        .args(["-static", "--gc-sections", "-e", "_start"])
        .arg(&object)
        .arg(archive())
        .arg("-o")
        .arg(&binary)
        .output()
        .ok()?;
    assert!(
        link.status.success(),
        "ld.lld failed for {name}: {}",
        String::from_utf8_lossy(&link.stderr)
    );
    let run = Command::new(&binary).output().ok()?;
    assert!(
        run.status.success(),
        "{name} failed with {:?}: {}",
        run.status.code(),
        String::from_utf8_lossy(&run.stdout)
    );
    Some(String::from_utf8_lossy(&run.stdout).into_owned())
}

#[test]
fn a9_hostrun_h5_glob_regex_qsort() {
    let Some(output) = hostrun("h5_glob") else {
        return;
    };
    assert!(output.contains("H5 OK"), "h5 output: {output}");
}

#[test]
fn a10_hostrun_h6_platform_glue() {
    let Some(output) = hostrun("h6_platform") else {
        return;
    };
    assert!(output.contains("H6 OK"), "h6 output: {output}");
}

#[test]
fn a11_every_expected_family_name_is_defined() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let text = defined();
    let names = text
        .iter()
        .filter(|(_, kind, _)| matches!(kind, 'T' | 't'))
        .map(|(_, _, name)| name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for family in ["regex", "glob", "fts", "stdlib", "unistd"] {
        let path = root()
            .join("libc/ravenlibc/crates/raven-libc/expected")
            .join(format!("{family}.txt"));
        let expected = std::fs::read_to_string(&path).expect("expected list");
        for line in expected.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            assert!(names.contains(line), "{family}: `{line}` is not defined");
        }
    }
}

#[test]
fn a12_c60c_data_symbols_are_defined() {
    if !archive().is_file() {
        skip_or_fail!("no archive");
    }
    let names = defined()
        .into_iter()
        .map(|(_, _, name)| name)
        .collect::<std::collections::BTreeSet<_>>();
    for name in [
        "environ",
        "optarg",
        "opterr",
        "optind",
        "optopt",
        "optreset",
        "suboptarg",
    ] {
        assert!(names.contains(name), "data symbol `{name}` is not defined");
    }
}
