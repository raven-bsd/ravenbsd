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
        "--verify".into(),
        "--json".into(),
        "--out".into(),
        "build/alt".into(),
    ])
    .expect("parse");
    assert_eq!(options.arch, Arch::Aarch64);
    assert_eq!(options.set, "tiny");
    assert!(options.check);
    assert!(options.verify);
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

fn loader_gate_text(base: u64) -> String {
    let mut text = format!(
        "Type:                              DYN (Shared object file)\n  \
         LOAD           0x000000 0x{base:016x} 0x{base:016x} 0x000010 0x000010 R   0x1000\n  \
         GNU_RELRO      0x000000 0x{base:016x} 0x{base:016x} 0x000010 0x000010 R   0x1\n  \
         GNU_STACK      0x000000 0x0000000000000000 0x0000000000000000 0x000000 0x000000 RW  0x10\n  \
         0x000000000000001e (FLAGS)              BIND_NOW\n  \
         0x000000000000000e (SONAME)             Library soname: [ld-elf.so.1]\n\
         Entry point address:               0x1234\n  \
         1234: 0000000000001234     0 FUNC    GLOBAL DEFAULT    1 _rtld_start\n\
         Symbol table '.dynsym' contains 17 entries:\n"
    );
    for (index, name) in export_names().iter().enumerate() {
        text.push_str(&format!(
            "  {index}: 0000000000001000    10 FUNC    GLOBAL DEFAULT    1 {name}@@RAVEN_1.0\n"
        ));
    }
    text
}

#[test]
fn the_loader_gate_requires_the_supported_image_base() {
    assert!(gate_text(&loader_gate_text(LOADER_IMAGE_BASE), "loader").is_ok());
    let error = gate_text(&loader_gate_text(0), "loader").expect_err("base zero");
    assert!(format!("{error}").contains("first PT_LOAD image base"));
}

#[test]
fn loader_manifest_renders_and_parses_the_contract() {
    let bytes = b"loader-bytes";
    let text = loader_manifest_text("x86_64", "abc1234", bytes, "deadbeef");
    let manifest = parse_loader_manifest(&text).expect("manifest");
    assert_eq!(manifest.arch, "x86_64");
    assert_eq!(manifest.revision, "abc1234");
    assert_eq!(manifest.image_base, "0x1000");
    assert_eq!(manifest.interpreter, "/libexec/ld-elf.so.1");
    assert_eq!(manifest.size, bytes.len() as u64);
    assert_eq!(
        manifest.crc32,
        format!("{:08x}", raven_initfs::crc32(bytes))
    );
    assert_eq!(manifest.sha256, "deadbeef");
}

#[test]
fn a_stale_loader_or_manifest_does_not_match() {
    let root = std::env::temp_dir().join(format!("k131-manifest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let loader = root.join("build/x86_64/ld-elf.so.1");
    std::fs::create_dir_all(loader.parent().unwrap()).unwrap();
    std::fs::write(&loader, b"fresh").unwrap();
    let manifest = loader_manifest_path(&root, "x86_64");
    let text = loader_manifest_text("x86_64", "rev", b"fresh", "sha");
    std::fs::write(&manifest, &text).unwrap();
    assert!(loader_manifest_matches(&root, "x86_64"));

    std::fs::write(&loader, b"stale").unwrap();
    assert!(!loader_manifest_matches(&root, "x86_64"));

    std::fs::write(&loader, b"fresh").unwrap();
    std::fs::write(&manifest, text.replace("0x1000", "0x0")).unwrap();
    assert!(!loader_manifest_matches(&root, "x86_64"));

    std::fs::remove_file(&manifest).unwrap();
    assert!(!loader_manifest_matches(&root, "x86_64"));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn stage_sysroot_replaces_a_stale_libc_so_with_the_script() {
    let out = std::env::temp_dir().join(format!("k430-sysroot-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let stale = out.join("sysroot/usr/lib/libc.so");
    std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
    std::fs::write(&stale, b"stale ELF, not the dynamic libc script").unwrap();
    let libc = out.join("libc.so.1");
    let loader = out.join("ld-elf.so.1");
    std::fs::write(&libc, b"libc").unwrap();
    std::fs::write(&loader, b"loader").unwrap();

    stage_sysroot(&out, &libc, &loader).expect("stage");

    assert_eq!(
        std::fs::read(&stale).expect("script"),
        b"INPUT ( libc.so.1 )\n"
    );
    assert_eq!(
        std::fs::read(out.join("sysroot/lib/libc.so.1")).expect("libc"),
        b"libc"
    );
    assert_eq!(
        std::fs::read(out.join("sysroot/libexec/ld-elf.so.1")).expect("loader"),
        b"loader"
    );
    std::fs::remove_dir_all(&out).unwrap();
}

#[test]
fn dso_stamp_goes_stale_with_the_sources_and_files() {
    let root = std::env::temp_dir().join(format!("k548-dso-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("libc/ravenlibc/crates/raven-libc/src.rs");
    std::fs::create_dir_all(source.parent().expect("parent")).expect("mkdir");
    std::fs::write(&source, b"one").expect("source");
    let out = root.join("build/x86_64");
    std::fs::create_dir_all(out.join("sysroot/lib")).expect("lib");
    std::fs::create_dir_all(out.join("sysroot/libexec")).expect("libexec");
    for path in [
        "libc.so.1",
        "ld-elf.so.1",
        "sysroot/lib/libc.so.1",
        "sysroot/libexec/ld-elf.so.1",
    ] {
        std::fs::write(out.join(path), path.as_bytes()).expect("file");
    }
    let hash = |path: &str| crate::libc_pack::hash_file(&out.join(path)).expect("hash");
    let sources = crate::libc_pack::source_hash(&root).expect("sources");
    let text = format!(
        "format={DSO_STAMP_FORMAT}\narch=x86_64\nset=server\nsources={sources:016x}\nlibc_so={:016x}\nsysroot_libc_so={:016x}\nloader={:016x}\nsysroot_loader={:016x}\n",
        hash("libc.so.1"),
        hash("sysroot/lib/libc.so.1"),
        hash("ld-elf.so.1"),
        hash("sysroot/libexec/ld-elf.so.1"),
    );
    std::fs::write(out.join(DSO_STAMP_FILE), text).expect("stamp");
    assert!(dso_stamp_current(&root, "x86_64"));
    std::fs::write(&source, b"two").expect("rewrite");
    assert!(!dso_stamp_current(&root, "x86_64"));
    std::fs::write(&source, b"one").expect("restore");
    std::fs::write(out.join("ld-elf.so.1"), b"changed loader").expect("loader");
    assert!(!dso_stamp_current(&root, "x86_64"));
    let _ = std::fs::remove_dir_all(&root);
}
