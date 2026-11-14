// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Structure of the eight start objects and the C61 section 4.7 checks, plus source-level
//! checks of `start.rs` and the `.fini_array` hook. The E1 path (staticlib + member extraction)
//! and the E2 fallback (one `rustc --emit=obj` run) are both produced and compared.
//!
//! Host-only: the objects are `x86_64-unknown-none`; on any other host the tests print
//! `SKIP: host`. `RL_LIBC_REQUIRED=1` turns a skip into a failure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const TARGET: &str = "x86_64-unknown-none";

/// The eight objects, in the order `libc-pack` writes them.
const OBJECTS: [&str; 8] = [
    "crt1",
    "Scrt1",
    "rcrt1",
    "crti",
    "crtn",
    "crtbegin",
    "crtbeginS",
    "crtend",
];

struct Built {
    objects: BTreeMap<String, PathBuf>,
    e2_crt1: PathBuf,
}

impl Built {
    fn path(&self, name: &str) -> &Path {
        self.objects
            .get(name)
            .unwrap_or_else(|| panic!("no {name}.o"))
    }
}

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
    base.join("c61-objects")
}

fn require_host() -> bool {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        println!("SKIP: host");
        return false;
    }
    true
}

fn find_tool(names: &[&str]) -> Option<PathBuf> {
    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())?;
    let host = Command::new("rustc")
        .args(["-vV"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())?;
    let host = host
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
        .unwrap_or_else(|| "x86_64-unknown-linux-gnu".to_string());
    let bin = Path::new(&sysroot)
        .join("lib/rustlib")
        .join(host)
        .join("bin");
    for name in names {
        let candidate = bin.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for name in names {
        for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn build_objects(tag: &str) -> Result<Built, String> {
    let root = repo_root();
    let dir = work_dir().join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("members")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(dir.join("objects")).map_err(|error| error.to_string())?;
    let archive = dir.join("libraven_libc_crt.a");
    let status = Command::new("rustc")
        .args(["--edition", "2024", "--crate-name", "raven_libc_crt"])
        .args(["--crate-type", "staticlib"])
        .args(["--target", TARGET])
        .args([
            "-C",
            "panic=abort",
            "-C",
            "opt-level=z",
            "-C",
            "codegen-units=4096",
            "-C",
            "relocation-model=pic",
        ])
        .arg("-o")
        .arg(&archive)
        .arg(root.join("libc/ravenlibc/crt/src/lib.rs"))
        .status()
        .map_err(|error| format!("cannot run rustc: {error}"))?;
    if !status.success() {
        return Err("the crt staticlib build failed".to_string());
    }

    let ar = find_tool(&["llvm-ar", "ar"]).ok_or_else(|| "no ar".to_string())?;
    let nm = find_tool(&["llvm-nm", "nm"]).ok_or_else(|| "no nm".to_string())?;
    let objcopy =
        find_tool(&["llvm-objcopy", "objcopy"]).ok_or_else(|| "no objcopy".to_string())?;
    let listing = Command::new(&ar)
        .args(["t"])
        .arg(&archive)
        .output()
        .map_err(|error| error.to_string())?;
    let names = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|line| line.contains(".raven_libc_crt."))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if names.is_empty() {
        return Err("no raven_libc_crt members".to_string());
    }
    let mut extract = Command::new(&ar);
    extract.arg("x").arg(&archive);
    for name in &names {
        extract.arg(name);
    }
    let status = extract
        .current_dir(dir.join("members"))
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err("ar x failed".to_string());
    }

    let mut objects = BTreeMap::new();
    for object in OBJECTS {
        let source = match object {
            "Scrt1" => "scrt1",
            "crtbeginS" => "crtbegin_s",
            other => other,
        };
        let marker = format!("__raven_crt_kind_{source}");
        let mut owners = Vec::new();
        for name in &names {
            let path = dir.join("members").join(name);
            let output = Command::new(&nm)
                .args(["--defined-only", "--format=posix"])
                .arg(&path)
                .output()
                .map_err(|error| error.to_string())?;
            let text = String::from_utf8_lossy(&output.stdout);
            if text.lines().any(|line| {
                line.split_whitespace()
                    .next()
                    .is_some_and(|symbol| symbol == marker)
            }) {
                owners.push(path);
            }
        }
        if owners.len() != 1 {
            return Err(format!(
                "{object}: {} members define the marker",
                owners.len()
            ));
        }
        let output = dir.join("objects").join(format!("{object}.o"));
        std::fs::copy(&owners[0], &output).map_err(|error| error.to_string())?;
        strip(&objcopy, &nm, &output, &marker)?;
        if matches!(object, "crti" | "crtn") {
            let next = output.with_extension("textless.o");
            let status = Command::new(&objcopy)
                .arg("--remove-section=.text")
                .arg(&output)
                .arg(&next)
                .status()
                .map_err(|error| error.to_string())?;
            if !status.success() {
                return Err("cannot remove the empty .text".to_string());
            }
            std::fs::rename(&next, &output).map_err(|error| error.to_string())?;
        }
        objects.insert(object.to_string(), output);
    }

    // The E2 fallback for crt1, through the repository's fallback root.
    let e2 = dir.join("objects").join("crt1-e2.o");
    let status = Command::new("rustc")
        .args([
            "--edition",
            "2024",
            "--crate-name",
            "crt1",
            "--crate-type",
            "lib",
            "--emit=obj",
        ])
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
        .arg(&e2)
        .arg(root.join("libc/ravenlibc/crt/objs/crt1.rs"))
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err("the E2 crt1 build failed".to_string());
    }
    strip(&objcopy, &nm, &e2, "__raven_crt_kind_crt1")?;
    Ok(Built {
        objects,
        e2_crt1: e2,
    })
}

fn strip(objcopy: &Path, nm: &Path, object: &Path, marker: &str) -> Result<(), String> {
    let cleaned = object.with_extension("clean.o");
    let status = Command::new(objcopy)
        .args(["--remove-section", ".comment"])
        .arg(format!("--strip-symbol={marker}"))
        .arg(object)
        .arg(&cleaned)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err("objcopy failed".to_string());
    }
    let output = Command::new(nm)
        .args(["-a", "--format=posix"])
        .arg(&cleaned)
        .output()
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut current = cleaned;
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 2 && fields[1] == "a" && fields[0].contains("-cgu.") {
            let next = object.with_extension("clean2.o");
            let status = Command::new(objcopy)
                .arg(format!("--strip-symbol={}", fields[0]))
                .arg(&current)
                .arg(&next)
                .status()
                .map_err(|error| error.to_string())?;
            if !status.success() {
                return Err("cannot strip the FILE symbol".to_string());
            }
            current = next;
        }
    }
    std::fs::rename(&current, object).map_err(|error| error.to_string())?;
    Ok(())
}

fn built() -> Option<&'static Built> {
    static BUILT: OnceLock<Result<Built, String>> = OnceLock::new();
    match BUILT.get_or_init(|| build_objects("e1")) {
        Ok(built) => Some(built),
        Err(error) => {
            if std::env::var("RL_LIBC_REQUIRED").as_deref() == Ok("1") {
                panic!("required object build failed: {error}");
            }
            println!("SKIP: {error}");
            None
        }
    }
}

fn run(program: &Path, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("cannot run {}: {error}", program.display()));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn defined(path: &Path) -> Vec<(String, String)> {
    let Some(nm) = find_tool(&["llvm-nm", "nm"]) else {
        return Vec::new();
    };
    let output = Command::new(nm)
        .args(["--defined-only", "--format=posix"])
        .arg(path)
        .output()
        .expect("cannot run nm");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            Some((fields.first()?.to_string(), fields.get(1)?.to_string()))
        })
        .collect()
}

#[test]
fn entry_objects_define_the_entry_symbol() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    for object in ["crt1", "Scrt1"] {
        let symbols = defined(built.path(object));
        let text = symbols
            .iter()
            .filter(|(_, kind)| kind == "T")
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(text, ["_start"], "{object}.o");
    }
    let symbols = defined(built.path("rcrt1"));
    let text = symbols
        .iter()
        .filter(|(_, kind)| kind == "T")
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert!(text.contains(&"_start"), "rcrt1.o has no _start");
    assert!(
        text.contains(&"__raven_rcrt1_stage1"),
        "rcrt1.o has no stage-1 function"
    );
    assert_eq!(text.len(), 2, "rcrt1.o text symbols: {text:?}");
}

#[test]
fn crti_and_crtn_define_nothing() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    for object in ["crti", "crtn"] {
        assert!(defined(built.path(object)).is_empty(), "{object}.o");
    }
}

#[test]
fn every_object_has_a_non_executable_stack_note() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let Some(readelf) = find_tool(&["llvm-readelf", "readelf"]) else {
        println!("SKIP: no readelf");
        return;
    };
    for object in OBJECTS {
        let path = built.path(object);
        let sections = run(&readelf, &["-SW", path.to_str().unwrap()]);
        assert!(sections.contains(".note.GNU-stack"), "{object}.o");
        for line in sections.lines() {
            if line.contains(".note.GNU-stack") {
                assert!(
                    !line.split_whitespace().any(|word| word == "X"),
                    "{object}.o executable stack"
                );
            }
            for forbidden in [".init", ".fini", ".ctors", ".init_array", ".preinit_array"] {
                assert!(
                    !line
                        .split_whitespace()
                        .any(|word| word == forbidden || word.starts_with(&format!("{forbidden}."))),
                    "{object}.o has {forbidden}"
                );
            }
        }
    }
}

#[test]
fn entry_objects_carry_the_abi_note() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let Some(readelf) = find_tool(&["llvm-readelf", "readelf"]) else {
        println!("SKIP: no readelf");
        return;
    };
    for object in ["crt1", "Scrt1", "rcrt1"] {
        let path = built.path(object);
        let sections = run(&readelf, &["-SW", path.to_str().unwrap()]);
        assert!(sections.contains(".note.raven.abi"), "{object}.o");
        let notes = run(&readelf, &["-n", path.to_str().unwrap()]);
        assert!(notes.contains("RavenBSD"), "{object}.o owner");
        assert!(notes.contains("01 00 00 00"), "{object}.o descriptor");
    }
}

#[test]
fn rcrt1_has_no_got_class_relocation() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let Some(readelf) = find_tool(&["llvm-readelf", "readelf"]) else {
        println!("SKIP: no readelf");
        return;
    };
    let path = built.path("rcrt1");
    let relocations = run(&readelf, &["-rW", path.to_str().unwrap()]);
    for line in relocations.lines() {
        let upper = line.to_ascii_uppercase();
        assert!(
            !upper.contains("GOT") && !upper.contains("GLOB_DAT") && !upper.contains("JUMP_SLOT"),
            "rcrt1.o GOT relocation: {}",
            line.trim()
        );
    }
}

#[test]
fn only_crt1_references_the_static_iplt_range() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let Some(nm) = find_tool(&["llvm-nm", "nm"]) else {
        println!("SKIP: no nm");
        return;
    };
    let crt1 = run(&nm, &[built.path("crt1").to_str().unwrap()]);
    assert!(crt1.contains("__rela_iplt_start"), "crt1 start");
    assert!(crt1.contains("__rela_iplt_end"), "crt1 end");
    for object in ["Scrt1", "rcrt1"] {
        let symbols = run(&nm, &[built.path(object).to_str().unwrap()]);
        assert!(!symbols.contains("__rela_iplt_start"), "{object}");
        assert!(!symbols.contains("__rela_iplt_end"), "{object}");
    }
}

#[test]
fn crtbegin_s_and_crtend_have_their_sections() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let Some(readelf) = find_tool(&["llvm-readelf", "readelf"]) else {
        println!("SKIP: no readelf");
        return;
    };
    let symbols = defined(built.path("crtbeginS"));
    assert!(symbols.iter().any(|(name, _)| name == "__dso_handle"));
    assert!(symbols.iter().any(|(name, _)| name == "__do_fini"));
    let sections = run(
        &readelf,
        &["-SW", built.path("crtbeginS").to_str().unwrap()],
    );
    assert!(sections.contains(".fini_array"));
    let hidden = run(
        &readelf,
        &["-sW", built.path("crtbeginS").to_str().unwrap()],
    );
    assert!(hidden.contains("HIDDEN"), "__dso_handle is not hidden");
    let end = run(&readelf, &["-SW", built.path("crtend").to_str().unwrap()]);
    assert!(end.contains(".eh_frame"), "crtend.o has no .eh_frame");
}

#[test]
fn e1_and_e2_objects_are_byte_identical() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let e1 = std::fs::read(built.path("crt1")).expect("E1 object");
    let e2 = std::fs::read(&built.e2_crt1).expect("E2 object");
    assert_eq!(e1, e2, "the E1 and E2 crt1 objects differ");
}

#[test]
fn two_staticlib_builds_are_byte_identical() {
    if !require_host() {
        return;
    }
    let Some(first) = built() else { return };
    let second = match build_objects("reproducible") {
        Ok(built) => built,
        Err(error) => {
            println!("SKIP: {error}");
            return;
        }
    };
    for object in OBJECTS {
        let a = std::fs::read(first.path(object)).expect("first object");
        let b = std::fs::read(second.path(object)).expect("second object");
        assert_eq!(a, b, "{object}.o is not reproducible");
    }
}

#[test]
fn comment_section_is_stripped() {
    if !require_host() {
        return;
    }
    let Some(built) = built() else { return };
    let Some(readelf) = find_tool(&["llvm-readelf", "readelf"]) else {
        println!("SKIP: no readelf");
        return;
    };
    for object in OBJECTS {
        let sections = run(&readelf, &["-SW", built.path(object).to_str().unwrap()]);
        assert!(!sections.contains(".comment"), "{object}.o keeps .comment");
    }
}

// ------------------------------------------------------------------- start.rs

fn start_source() -> String {
    std::fs::read_to_string(repo_root().join("libc/ravenlibc/crates/raven-libc/src/start.rs"))
        .expect("cannot read start.rs")
}

#[test]
fn start_steps_appear_in_numeric_order() {
    let source = start_source();
    let mut steps = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("// STEP ") {
            let number = rest
                .split(|character: char| !character.is_ascii_digit())
                .next()
                .and_then(|digits| digits.parse::<u32>().ok());
            if let Some(number) = number
                && !steps.contains(&number)
            {
                steps.push(number);
            }
        }
    }
    assert_eq!(steps, (1..=15).collect::<Vec<_>>(), "step order: {steps:?}");
}

#[test]
fn start_has_no_forbidden_work() {
    let source = start_source();
    for forbidden in ["malloc", "fopen", "setlocale", "getenv"] {
        assert!(!source.contains(forbidden), "start.rs mentions {forbidden}");
    }
}

#[test]
fn start_uses_the_four_argument_convention() {
    let source = start_source();
    for argument in ["sp0", "rtld_fini", "iplt_start", "iplt_end"] {
        assert!(source.contains(argument), "missing {argument}");
    }
    assert!(source.contains("__raven_init_main_thread"));
    assert!(source.contains("register_cxa"));
    assert!(source.contains("__rl_fini_array"));
}

#[test]
fn start_auxv_table_is_a_stack_array_with_raven_slots() {
    let source = start_source();
    assert!(source.contains("[0usize; 64]"));
    assert!(source.contains("RAVEN_SLOT_BASE"));
    assert!(source.contains("0x7000"));
}

#[test]
fn start_calls_exit_with_main() {
    let source = start_source();
    assert!(source.contains("crate::stdlib::exit::exit(code)"));
    assert!(source.contains("main(argc as c_int, argv, envp)"));
}

#[test]
fn fini_array_runs_backwards_and_skips_placeholders() {
    let source = std::fs::read_to_string(
        repo_root().join("libc/ravenlibc/crates/raven-libc/src/hooks/fini_array.rs"),
    )
    .expect("cannot read fini_array.rs");
    assert!(source.contains("cursor.wrapping_sub(1)"));
    for placeholder in ["entry != 0", "entry != 1", "usize::MAX"] {
        assert!(source.contains(placeholder), "missing {placeholder}");
    }
    assert!(source.contains("__rl_fini_array"));
}

#[test]
fn fini_array_is_in_the_hook_table_and_symbol_map() {
    let hooks =
        std::fs::read_to_string(repo_root().join("libc/ravenlibc/crates/raven-libc/hooks.toml"))
            .expect("cannot read hooks.toml");
    assert!(hooks.contains("__rl_fini_array"));
    let map =
        std::fs::read_to_string(repo_root().join("libc/ravenlibc/crates/raven-libc/Symbol.map"))
            .expect("cannot read Symbol.map");
    assert!(map.contains("__raven_libc_start;"));
    assert!(map.contains("__rl_fini_array;"));
}

#[test]
fn crt_crate_declares_eight_asm_modules() {
    let source = std::fs::read_to_string(repo_root().join("libc/ravenlibc/crt/src/lib.rs"))
        .expect("cannot read the crt lib.rs");
    for module in [
        "crt1",
        "scrt1",
        "rcrt1",
        "crti",
        "crtn",
        "crtbegin",
        "crtbegin_s",
        "crtend",
    ] {
        assert!(
            source.contains(&format!("mod {module};")),
            "missing {module}"
        );
    }
    assert!(source.contains("pub mod selfreloc;"));
    assert!(source.contains("pub mod selfreloc_consts;"));
}
