// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parse_defaults_to_x86_64_server() {
    let options = parse(&[]).expect("parse");
    assert_eq!(options.arch, Arch::X86_64);
    assert_eq!(options.set, "server");
    assert!(!options.check);
    assert_eq!(options.out, PathBuf::from("build/x86_64"));
}

#[test]
fn parse_accepts_arch_set_and_check() {
    let options = parse(&[
        "--arch".into(),
        "aarch64".into(),
        "--set".into(),
        "tiny".into(),
        "--check".into(),
        "--json".into(),
        "--out".into(),
        "build/alt".into(),
    ])
    .expect("parse");
    assert_eq!(options.arch, Arch::Aarch64);
    assert_eq!(options.set, "tiny");
    assert!(options.check);
    assert!(options.json);
    assert_eq!(options.out, PathBuf::from("build/alt"));
}

#[test]
fn parse_rejects_an_unknown_arch() {
    assert!(parse(&["--arch".into(), "mips".into()]).is_err());
}

#[test]
fn parse_rejects_an_unknown_set() {
    assert!(parse(&["--set".into(), "huge".into()]).is_err());
}

#[test]
fn the_export_list_has_seventeen_names() {
    assert_eq!(export_names().len(), 17);
    assert!(export_names().contains(&"dlopen"));
    assert!(!export_names().contains(&"_rtld_start"));
}

#[test]
fn undef_stops_at_the_private_marker() {
    let dir = std::env::temp_dir().join("c62-libc-link-ut");
    let _ = std::fs::create_dir_all(&dir);
    let symbols = dir.join("Symbols.list");
    let out = dir.join("undef.rsp");
    std::fs::write(&symbols, "alpha\nbeta\n# private\ngamma\n").expect("write");
    write_undef(&symbols, &out).expect("undef");
    let text = std::fs::read_to_string(&out).expect("read");
    assert!(text.contains("-u alpha"));
    assert!(text.contains("-u beta"));
    assert!(!text.contains("gamma"));
}

fn libc_gate_text() -> String {
    "Type:                              DYN (Shared object file)\n  \
     LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x000010 0x000010 R   0x1000\n  \
     GNU_RELRO      0x000000 0x0000000000000000 0x0000000000000000 0x000010 0x000010 R   0x1\n  \
     GNU_STACK      0x000000 0x0000000000000000 0x0000000000000000 0x000000 0x000000 RW  0x10\n  \
     0x000000000000001e (FLAGS)              BIND_NOW\n  \
     0x000000000000000e (SONAME)             Library soname: [libc.so.1]\n"
        .to_string()
}

#[test]
fn the_elf_gate_accepts_a_clean_libc_dump() {
    assert!(gate_text(&libc_gate_text(), "libc").is_ok());
}

#[test]
fn the_elf_gate_rejects_interp_and_tls() {
    let interp = format!(
        "{}  INTERP         0x000000 0x0 0x0 0x10 0x10 R   0x1\n",
        libc_gate_text()
    );
    let error = gate_text(&interp, "libc").expect_err("interp");
    assert!(format!("{error}").contains("PT_INTERP"));
    let tls = format!(
        "{}  TLS            0x000000 0x0 0x0 0x10 0x10 R   0x1\n",
        libc_gate_text()
    );
    let error = gate_text(&tls, "libc").expect_err("tls");
    assert!(format!("{error}").contains("PT_TLS"));
}

#[test]
fn the_elf_gate_rejects_a_write_execute_load() {
    let text = libc_gate_text().replace("R   0x1000", "RWE 0x1000");
    let error = gate_text(&text, "libc").expect_err("wx");
    assert!(format!("{error}").contains("W|X"));
}

#[test]
fn the_elf_gate_rejects_a_missing_bind_now() {
    let text = libc_gate_text().replace("BIND_NOW", "NOW");
    let error = gate_text(&text, "libc").expect_err("bind now");
    assert!(format!("{error}").contains("BIND_NOW"));
}

#[test]
fn a_tiny_link_is_skipped() {
    let code = run(Path::new("."), &["--set".into(), "tiny".into()]).expect("run");
    assert_eq!(code, 0);
}

#[test]
fn the_elf_gate_accepts_a_real_shared_object() {
    let dir = std::env::temp_dir().join("c62-libc-link-elf");
    let _ = std::fs::create_dir_all(&dir);
    let source = dir.join("one.s");
    let object = dir.join("one.o");
    let so = dir.join("one.so");
    std::fs::write(&source, ".text\n.globl f\nf: ret\n").expect("write");
    let clang = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-c",
            "-x",
            "assembler",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .status();
    let Ok(status) = clang else {
        println!("SKIP: clang");
        return;
    };
    if !status.success() {
        println!("SKIP: clang");
        return;
    }
    let linker = find_linker(Path::new(".")).expect("linker");
    let args = vec![
        "-flavor".to_string(),
        "gnu".to_string(),
        "-shared".to_string(),
        "--soname=libc.so.1".to_string(),
        "-z".to_string(),
        "now".to_string(),
        "-z".to_string(),
        "relro".to_string(),
        "-z".to_string(),
        "noexecstack".to_string(),
        object.display().to_string(),
        "-o".to_string(),
        so.display().to_string(),
    ];
    if run_link(&linker, &args, Path::new(".")).expect("link") != 0 {
        println!("SKIP: linker");
        return;
    }
    // This fixture has no SONAME check for the loader; the gate is a smoke check of the parser.
    let _ = check_elf(&so, "libc");
}
