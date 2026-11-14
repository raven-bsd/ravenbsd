// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `libc-stubs` generator (ticket C66 steps 7-9: determinism, T9 links and the
//! feature-macro acceptance matrix).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .to_path_buf()
}

fn table() -> Stubs {
    Stubs::load(&root().join("libc/ravenlibc/stubs/stubs.toml")).expect("stubs.toml")
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("raven-c66-{}-{serial}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn required() -> bool {
    std::env::var("RL_STUBS_REQUIRED").as_deref() == Ok("1")
}

fn skip_or_fail(reason: &str) {
    if required() {
        panic!("SKIP: {reason} (RL_STUBS_REQUIRED=1)");
    }
    eprintln!("SKIP: {reason}");
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[test]
fn table_has_the_twenty_names() {
    let table = table();
    let names: Vec<&str> = table.stub.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, EXPECTED);
}

#[test]
fn table_rejects_unknown_keys() {
    let text =
        "[[stub]]\nname=\"m\"\nring=1\ncontent=\"x\"\nalias_soname=\"\"\nnote=\"\"\nextra=1\n";
    assert!(toml::from_str::<Stubs>(text).is_err());
}

#[test]
fn table_rejects_a_missing_name() {
    let text = "[[stub]]\nname=\"md\"\nring=2\ncontent=\"x\"\nalias_soname=\"\"\nnote=\"\"\n";
    let parsed: Stubs = toml::from_str(text).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn outputs_are_forty_files_with_exact_bytes() {
    let files = outputs(&table());
    assert_eq!(files.len(), 40);
    let archives = files
        .iter()
        .filter(|file| file.name.ends_with(".a"))
        .count();
    let scripts = files
        .iter()
        .filter(|file| file.name.ends_with(".so"))
        .count();
    assert_eq!(archives, 20);
    assert_eq!(scripts, 20);
    for file in &files {
        if file.name.ends_with(".a") {
            assert_eq!(file.bytes, b"!<arch>\n");
            assert_eq!(file.bytes.len(), 8);
        } else {
            assert_eq!(file.bytes, b"INPUT(libc.so.1)\n");
            assert_eq!(file.bytes.len(), 17);
        }
    }
}

#[test]
fn no_forbidden_stub_is_generated() {
    let files = outputs(&table());
    for forbidden in [
        "libmd.a",
        "libmd.so",
        "libuuid.a",
        "libuuid.so",
        "libc_heap_tiny.a",
    ] {
        assert!(!files.iter().any(|file| file.name == forbidden));
    }
}

#[test]
fn generate_twice_is_byte_identical() {
    let temp = Temp::new("determinism");
    let table = table();
    assert_eq!(generate(&table, temp.path()).unwrap(), 40);
    let first: Vec<(String, Vec<u8>)> = outputs(&table)
        .into_iter()
        .map(|file| {
            (
                file.name.clone(),
                fs::read(temp.path().join(&file.name)).unwrap(),
            )
        })
        .collect();
    assert_eq!(generate(&table, temp.path()).unwrap(), 40);
    for (name, bytes) in first {
        assert_eq!(fs::read(temp.path().join(&name)).unwrap(), bytes, "{name}");
    }
}

#[test]
fn check_passes_on_fresh_output() {
    let temp = Temp::new("check-ok");
    let table = table();
    generate(&table, temp.path()).unwrap();
    assert!(check(&table, temp.path()).unwrap().is_empty());
}

#[test]
fn check_detects_a_changed_byte() {
    let temp = Temp::new("check-byte");
    let table = table();
    generate(&table, temp.path()).unwrap();
    let path = temp.path().join("libm.so");
    fs::write(&path, b"INPUT(libc.so.2)\n").unwrap();
    assert!(!check(&table, temp.path()).unwrap().is_empty());
}

#[test]
fn check_detects_a_missing_file() {
    let temp = Temp::new("check-missing");
    let table = table();
    generate(&table, temp.path()).unwrap();
    fs::remove_file(temp.path().join("libpthread.a")).unwrap();
    assert!(!check(&table, temp.path()).unwrap().is_empty());
}

#[test]
fn check_detects_an_extra_stub() {
    let temp = Temp::new("check-extra");
    let table = table();
    generate(&table, temp.path()).unwrap();
    fs::write(temp.path().join("libmd.a"), b"!<arch>\n").unwrap();
    assert!(!check(&table, temp.path()).unwrap().is_empty());
}

#[test]
fn check_missing_directory_is_not_a_failure() {
    let temp = Temp::new("check-absent");
    let table = table();
    assert!(
        check(&table, &temp.path().join("not-there"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn run_rejects_an_unknown_flag() {
    let temp = Temp::new("run-flag");
    assert!(run(temp.path(), &["--nope".to_string()]).is_err());
}

#[test]
fn run_generates_the_default_layout() {
    let temp = Temp::new("run-generate");
    let out = temp.path().join("stubs");
    let args = vec![
        "--out".to_string(),
        out.display().to_string(),
        "--check".to_string(),
    ];
    assert_eq!(
        run(&root(), &["--out".to_string(), out.display().to_string()]).unwrap(),
        0
    );
    assert_eq!(run(&root(), &args).unwrap(), 0);
    assert_eq!(fs::read_dir(&out).unwrap().count(), 40);
}

// ---------------------------------------------------------------------------------------------
// Feature-macro acceptance

fn clang() -> Option<String> {
    if have("clang") {
        Some("clang".to_string())
    } else {
        None
    }
}

fn clang_resource(program: &str) -> Option<PathBuf> {
    let out = Command::new(program)
        .arg("-print-resource-dir")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Some(PathBuf::from(text).join("include"))
}

fn feature_command(program: &str, source: &Path, defines: &[&str]) -> Command {
    let base = root().join("libc/ravenlibc");
    let mut command = Command::new(program);
    command
        .arg("-fsyntax-only")
        .arg("--target=x86_64-unknown-none")
        .arg("-nostdinc")
        .arg("-isystem")
        .arg(base.join("include"))
        .arg("-isystem")
        .arg(base.join("arch/x86_64/include"))
        .arg("-isystem")
        .arg(base.join("abi/generated/include"));
    if let Some(resource) = clang_resource(program) {
        command.arg("-isystem").arg(resource);
    }
    command.arg("-Werror");
    for define in defines {
        command.arg(define);
    }
    command.arg(source);
    command
}

#[test]
fn eight_feature_macros_are_accepted() {
    let Some(program) = clang() else {
        skip_or_fail("clang");
        return;
    };
    let source = root().join("libc/ravenlibc/stubs/tests/features/probe.c");
    for define in [
        "-D_GNU_SOURCE",
        "-D_BSD_SOURCE",
        "-D_DEFAULT_SOURCE",
        "-D_ALL_SOURCE",
        "-D__EXTENSIONS__",
        "-D__BSD_VISIBLE=1",
        "-D_LARGEFILE64_SOURCE",
        "-D_REENTRANT",
    ] {
        let output = feature_command(&program, &source, &[define])
            .output()
            .expect("clang");
        assert!(
            output.status.success(),
            "{define}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn strict_posix_hides_strlcpy() {
    let Some(program) = clang() else {
        skip_or_fail("clang");
        return;
    };
    let source = root().join("libc/ravenlibc/stubs/tests/features/strict_probe.c");
    let output = feature_command(&program, &source, &["-D_POSIX_C_SOURCE=200809L"])
        .output()
        .expect("clang");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("strlcpy"));
}

#[test]
fn gnu_source_restores_strlcpy() {
    let Some(program) = clang() else {
        skip_or_fail("clang");
        return;
    };
    let source = root().join("libc/ravenlibc/stubs/tests/features/strict_probe.c");
    let output = feature_command(
        &program,
        &source,
        &["-D_POSIX_C_SOURCE=200809L", "-D_GNU_SOURCE"],
    )
    .output()
    .expect("clang");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn file_offset_bits_32_is_an_error() {
    let Some(program) = clang() else {
        skip_or_fail("clang");
        return;
    };
    let source = root().join("libc/ravenlibc/stubs/tests/features/probe.c");
    let output = feature_command(&program, &source, &["-D_FILE_OFFSET_BITS=32"])
        .output()
        .expect("clang");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("FILE_OFFSET_BITS"));
}

// ---------------------------------------------------------------------------------------------
// T9 links

struct LinkTools {
    cc: String,
    ar: String,
    ld: String,
    readelf: String,
}

fn link_tools() -> Option<LinkTools> {
    for tool in ["cc", "ar", "readelf"] {
        if !have(tool) {
            skip_or_fail(tool);
            return None;
        }
    }
    let ld = if have("ld.lld") {
        "ld.lld".to_string()
    } else if have("ld") {
        "ld".to_string()
    } else {
        skip_or_fail("ld or ld.lld");
        return None;
    };
    Some(LinkTools {
        cc: "cc".to_string(),
        ar: "ar".to_string(),
        ld,
        readelf: "readelf".to_string(),
    })
}

fn link_fixture(tools: &LinkTools, temp: &Path) -> Option<()> {
    fs::write(
        temp.join("lib.c"),
        "int fake_libc_symbol(void) { return 0; }\n",
    )
    .ok()?;
    fs::write(
        temp.join("main.c"),
        "int fake_libc_symbol(void);\nint main(void) { return fake_libc_symbol(); }\n",
    )
    .ok()?;
    for file in ["lib.c", "main.c"] {
        let status = Command::new(&tools.cc)
            .args(["-c", "-fPIC", "-o"])
            .arg(temp.join(file.replace(".c", ".o")))
            .arg(temp.join(file))
            .status()
            .ok()?;
        if !status.success() {
            return None;
        }
    }
    let status = Command::new(&tools.ar)
        .current_dir(temp)
        .args(["rcs", "libc.a", "lib.o"])
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let status = Command::new(&tools.cc)
        .current_dir(temp)
        .args([
            "-shared",
            "-Wl,-soname,libc.so.1",
            "-o",
            "libc.so.1",
            "lib.o",
        ])
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    Some(())
}

fn library_arguments(table: &Stubs) -> Vec<String> {
    let mut args = Vec::new();
    let mut names: Vec<&str> = table.stub.iter().map(|row| row.name.as_str()).collect();
    names.sort_unstable();
    for name in names {
        args.push(format!("-l{name}"));
    }
    args.push("-lc".to_string());
    args
}

#[test]
fn t9_dynamic_link_needs_only_libc() {
    let Some(tools) = link_tools() else { return };
    let temp = Temp::new("t9-dynamic");
    if link_fixture(&tools, temp.path()).is_none() {
        skip_or_fail("C toolchain fixture");
        return;
    }
    let table = table();
    let stubs = temp.path().join("stubs");
    generate(&table, &stubs).unwrap();
    let out = temp.path().join("t");
    let mut command = Command::new(&tools.ld);
    command
        .current_dir(temp.path())
        .args(["-o", "t", "main.o", "-L", "stubs", "-L", ".", "-e", "main"])
        .args(library_arguments(&table))
        .args(["-dynamic-linker", "/libexec/ld-elf.so.1"]);
    let status = command.status().expect("linker");
    if !status.success() {
        skip_or_fail("the fake libc fixture does not link with this linker");
        return;
    }
    let output = Command::new(&tools.readelf)
        .args(["-d", out.to_str().unwrap_or("t")])
        .output()
        .expect("readelf");
    let text = String::from_utf8_lossy(&output.stdout);
    let needed: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split("NEEDED").nth(1))
        .filter_map(|value| value.rsplit('[').next())
        .map(|value| value.trim_end_matches(']').trim())
        .collect();
    assert_eq!(needed, vec!["libc.so.1"], "{text}");
}

#[test]
fn t9_static_link_accepts_empty_archives() {
    let Some(tools) = link_tools() else { return };
    let temp = Temp::new("t9-static");
    if link_fixture(&tools, temp.path()).is_none() {
        skip_or_fail("C toolchain fixture");
        return;
    }
    let table = table();
    let stubs = temp.path().join("stubs");
    generate(&table, &stubs).unwrap();
    let mut command = Command::new(&tools.ld);
    command
        .current_dir(temp.path())
        .args([
            "-static", "-o", "ts", "main.o", "-L", "stubs", "-L", ".", "-e", "main",
        ])
        .args(library_arguments(&table));
    let status = command.status().expect("linker");
    assert!(
        status.success(),
        "static link with the empty stub archives failed"
    );
}

#[test]
fn t9_lld_accepts_empty_archives_when_present() {
    if !have("ld.lld") {
        skip_or_fail("ld.lld");
        return;
    }
    let tools = LinkTools {
        cc: "cc".to_string(),
        ar: "ar".to_string(),
        ld: "ld.lld".to_string(),
        readelf: "readelf".to_string(),
    };
    let temp = Temp::new("t9-lld");
    if link_fixture(&tools, temp.path()).is_none() {
        skip_or_fail("C toolchain fixture");
        return;
    }
    let table = table();
    let stubs = temp.path().join("stubs");
    generate(&table, &stubs).unwrap();
    let mut command = Command::new(&tools.ld);
    command
        .current_dir(temp.path())
        .args([
            "-static", "-o", "tl", "main.o", "-L", "stubs", "-L", ".", "-e", "main",
        ])
        .args(library_arguments(&table));
    assert!(command.status().expect("ld.lld").success());
}

#[test]
fn outputs_are_sorted_by_name() {
    let files = outputs(&table());
    let names: Vec<&str> = files.iter().map(|file| file.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
}

#[test]
fn archive_bytes_are_the_ar_magic() {
    assert_eq!(EMPTY_ARCHIVE, b"!<arch>\n");
    assert_eq!(SCRIPT, b"INPUT(libc.so.1)\n");
}

#[test]
fn validate_rejects_a_dotted_name() {
    let text = "[[stub]]\nname=\"md.so\"\nring=1\ncontent=\"x\"\nalias_soname=\"\"\nnote=\"\"\n";
    let parsed: Stubs = toml::from_str(text).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn validate_rejects_ring_zero() {
    let text = "[[stub]]\nname=\"m\"\nring=0\ncontent=\"x\"\nalias_soname=\"\"\nnote=\"\"\n";
    let parsed: Stubs = toml::from_str(text).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn validate_rejects_a_duplicate_name() {
    let row = "[[stub]]\nname=\"m\"\nring=1\ncontent=\"x\"\nalias_soname=\"\"\nnote=\"\"\n";
    let parsed: Stubs = toml::from_str(&format!("{row}{row}")).unwrap();
    assert!(parsed.validate().is_err());
}

#[test]
fn check_ignores_non_stub_files() {
    let temp = Temp::new("check-readme");
    let table = table();
    generate(&table, temp.path()).unwrap();
    fs::write(temp.path().join("README.md"), b"notes").unwrap();
    fs::write(temp.path().join("libm.pc"), b"prefix=/").unwrap();
    assert!(check(&table, temp.path()).unwrap().is_empty());
}

#[test]
fn generated_files_are_0644() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let temp = Temp::new("mode");
        generate(&table(), temp.path()).unwrap();
        let mode = fs::metadata(temp.path().join("libm.a"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }
}

#[test]
fn check_reports_every_expected_name() {
    let table = table();
    for row in &table.stub {
        assert!(EXPECTED.contains(&row.name.as_str()), "{}", row.name);
    }
    assert_eq!(table.stub.len(), 20);
}

#[test]
fn no_pc_file_is_generated() {
    let files = outputs(&table());
    assert!(!files.iter().any(|file| file.name.ends_with(".pc")));
}

#[test]
fn default_arch_names_are_known() {
    assert_eq!(Arch::parse("x86_64").unwrap(), Arch::X86_64);
    assert_eq!(Arch::parse("aarch64").unwrap(), Arch::Aarch64);
    assert_eq!(Arch::parse("riscv64").unwrap(), Arch::Riscv64);
    assert!(Arch::parse("mips").is_err());
}

#[test]
fn check_reports_a_truncated_archive() {
    let temp = Temp::new("check-truncated");
    let table = table();
    generate(&table, temp.path()).unwrap();
    fs::write(temp.path().join("libm.a"), b"!<arch>").unwrap();
    assert!(!check(&table, temp.path()).unwrap().is_empty());
}
