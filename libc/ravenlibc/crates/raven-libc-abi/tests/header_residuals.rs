// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S746 rev393 independent header/ABI checks for the three residual header gaps.
//!
//! These translation units are compiled by the host `clang` against the Raven public
//! include tree, never against the generated Rust model, so a table that agrees with
//! itself cannot make this pass. Expected facts are the pinned FreeBSD donor at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`:
//!
//! - `sys/sys/poll.h:93` defines `INFTIM (-1)` inside `#if __BSD_VISIBLE`.
//! - `sys/sys/resource.h:92` defines `struct __wrusage` as two `struct rusage` members;
//!   `sys/sys/wait.h:167` declares `wait6(idtype_t, id_t, int *, int,
//!   struct __wrusage *, siginfo_t *)`.
//! - `sys/sys/disklabel.h` includes `sys/ioccom.h` and `sys/disk/bsd.h`; the record is
//!   `148 + BSD_NPARTS_MIN * 16` bytes.

use std::path::PathBuf;
use std::process::Command;

/// FreeBSD donor revision pinned by `ravenbsd-resources/donors.lock`.
const DONOR_REV: &str = "133344405f532bd54ff3629a77d4c4e062f01fdb";

/// The two FreeBSD-profile architectures the oracle covers.
const ARCHES: [&str; 2] = ["x86_64", "aarch64"];

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../.."))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn clang() -> String {
    std::env::var("RAVEN_CLANG").unwrap_or_else(|_| "clang".to_string())
}

fn resource_include() -> PathBuf {
    let output = Command::new(clang())
        .args(["-print-resource-dir"])
        .output()
        .unwrap_or_else(|error| panic!("cannot run {}: {error}", clang()));
    assert!(
        output.status.success(),
        "clang -print-resource-dir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let dir = String::from_utf8_lossy(&output.stdout);
    PathBuf::from(dir.trim()).join("include")
}

fn include_args(arch: &str) -> Vec<String> {
    let root = repo_root();
    vec![
        format!("--target={arch}-unknown-ravenbsd"),
        "-nostdinc".to_string(),
        "-isystem".to_string(),
        resource_include().display().to_string(),
        "-isystem".to_string(),
        root.join("libc/ravenlibc/abi/generated/include")
            .display()
            .to_string(),
        "-isystem".to_string(),
        root.join("libc/ravenlibc/include").display().to_string(),
        "-isystem".to_string(),
        root.join(format!("libc/ravenlibc/arch/{arch}/include"))
            .display()
            .to_string(),
    ]
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("raven-s746-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|error| panic!("cannot create {}: {error}", dir.display()));
    dir
}

fn compile(arch: &str, tag: &str, source: &str, defines: &[&str]) -> Result<(), String> {
    let dir = temp_dir(tag);
    let path = dir.join(format!("{arch}-{tag}.c"));
    std::fs::write(&path, source)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
    let mut command = Command::new(clang());
    command.args(include_args(arch));
    command.args(["-fsyntax-only", "-std=gnu17"]);
    for define in defines {
        command.arg(format!("-D{define}"));
    }
    command.arg(&path);
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("cannot run {}: {error}", clang()));
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{arch} {tag} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn assert_all_arches(tag: &str, source: &str, defines: &[&str]) {
    for arch in ARCHES {
        compile(arch, tag, source, defines).unwrap_or_else(|error| panic!("{error}"));
    }
}

const INFTIM_PROBE: &str = r#"
#include <poll.h>
#include <sys/poll.h>

#ifndef INFTIM
#error INFTIM must be defined for the default (BSD-visible) environment
#endif
_Static_assert(INFTIM == -1, "FreeBSD sys/sys/poll.h defines INFTIM as (-1)");
_Static_assert(INFTIM < 0, "INFTIM selects an infinite poll timeout");
"#;

const INFTIM_HIDDEN_PROBE: &str = r#"
#include <poll.h>
#ifdef INFTIM
#error INFTIM must not leak into a strict environment
#endif
"#;

const WRUSAGE_PROBE: &str = r#"
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/resource.h>

_Static_assert(sizeof(struct __wrusage) == 2 * sizeof(struct rusage),
    "struct __wrusage has exactly the two struct rusage members");
_Static_assert(__builtin_offsetof(struct __wrusage, wru_self) == 0,
    "wru_self is the first member");
_Static_assert(__builtin_offsetof(struct __wrusage, wru_children) == sizeof(struct rusage),
    "wru_children follows wru_self with no padding");

_Static_assert(_Generic(&wait6,
        pid_t (*)(idtype_t, id_t, int *, int, struct __wrusage *, siginfo_t *): 1,
        default: 0),
    "wait6 has the FreeBSD sys/sys/wait.h prototype");
"#;

const DISKLABEL_PROBE: &str = r#"
/* Only sys/disklabel.h: the donor dependencies must arrive transitively. */
#include <sys/disklabel.h>

_Static_assert(DISKMAGIC == BSD_MAGIC, "disklabel uses the BSD magic");
_Static_assert(BSD_NPARTS_MIN == 8, "donor minimum partition count");
_Static_assert(sizeof(struct disklabel) == 148 + BSD_NPARTS_MIN * 16,
    "FreeBSD static record size assertion from sys/disk/bsd.h");
_Static_assert(sizeof(((struct disklabel *)0)->d_partitions[0]) == 16,
    "struct partition is 16 bytes");
_Static_assert(sizeof(((struct disklabel *)0)->d_partitions) == BSD_NPARTS_MIN * 16,
    "the default partition table holds BSD_NPARTS_MIN entries");
_Static_assert(_IOC(IOC_OUT, 'd', 1, sizeof(int)) == _IOR('d', 1, int),
    "sys/ioccom.h encodes ioctls");
"#;

#[test]
fn inftim_is_minus_one_and_bounded_by_bsd_visibility() {
    assert_all_arches("inftim-default", INFTIM_PROBE, &[]);
    assert_all_arches("inftim-gnu", INFTIM_PROBE, &["_GNU_SOURCE"]);
    assert_all_arches(
        "inftim-posix-hidden",
        INFTIM_HIDDEN_PROBE,
        &["_POSIX_C_SOURCE=200809L"],
    );
    assert_all_arches("inftim-ansi-hidden", INFTIM_HIDDEN_PROBE, &["_ANSI_SOURCE"]);
}

#[test]
fn wrusage_layout_and_wait6_prototype_match_the_freebsd_donor() {
    assert_all_arches("wrusage", WRUSAGE_PROBE, &[]);
}

#[test]
fn sys_disklabel_compiles_with_the_exact_donor_dependencies() {
    assert_all_arches("disklabel", DISKLABEL_PROBE, &[]);
}

#[test]
fn copied_headers_carry_bsd3_spdx_and_the_donor_revision() {
    for relative in [
        "libc/ravenlibc/include/sys/disklabel.h",
        "libc/ravenlibc/include/sys/ioccom.h",
        "libc/ravenlibc/include/sys/disk/bsd.h",
    ] {
        let text = read(relative);
        assert!(
            text.contains("SPDX-License-Identifier: BSD-3-Clause"),
            "{relative} lacks the BSD-3-Clause SPDX tag"
        );
        assert!(
            text.contains(DONOR_REV),
            "{relative} does not name the donor revision {DONOR_REV}"
        );
        let provenance = read("PROVENANCE.toml");
        assert!(
            provenance.contains(&format!("path = \"{relative}\"")),
            "PROVENANCE.toml has no import row for {relative}"
        );
    }
}

/// FreeBSD `sys/sys/fcntl.h:89,130` at the pinned donor revision.
const FCNTL_O_CLOEXEC: u64 = 0x0010_0000;
/// FreeBSD `sys/sys/fcntl.h:89` at the pinned donor revision.
const FCNTL_O_NONBLOCK: u64 = 0x0000_0004;

fn c_integer(text: &str) -> u64 {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else if text.len() > 1 && text.starts_with('0') {
        u64::from_str_radix(&text[1..], 8)
    } else {
        text.parse()
    }
    .unwrap_or_else(|error| panic!("cannot parse integer {text:?}: {error}"))
}

fn c_define(text: &str, name: &str) -> u64 {
    let prefix = format!("#define {name} ");
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no `#define {name}` in the C header"));
    c_integer(value.trim())
}

fn toml_value(text: &str, name: &str) -> u64 {
    let needle = format!("name = \"{name}\"");
    let line = text
        .lines()
        .find(|line| line.contains(&needle))
        .unwrap_or_else(|| panic!("no {name} row in fcntl.toml"));
    let rest = line
        .split("value = ")
        .nth(1)
        .unwrap_or_else(|| panic!("no value for {name} in fcntl.toml"));
    let value = rest.split(',').next().unwrap_or(rest).trim();
    c_integer(value)
}

fn rust_const(text: &str, name: &str) -> u64 {
    let prefix = format!("pub const {name}:");
    let line = text
        .lines()
        .find(|line| line.trim().starts_with(&prefix))
        .unwrap_or_else(|| panic!("no `pub const {name}` in the generated Rust module"));
    let rest = line
        .rsplit('=')
        .next()
        .unwrap_or_else(|| panic!("no value for {name}"));
    c_integer(rest.trim().trim_end_matches(';').trim())
}

fn assert_c_header_flags(origin: &str, text: &str) {
    assert_eq!(
        c_define(text, "O_CLOEXEC"),
        FCNTL_O_CLOEXEC,
        "{origin} O_CLOEXEC is not the FreeBSD pin 0x100000"
    );
    assert_eq!(
        c_define(text, "O_NONBLOCK"),
        FCNTL_O_NONBLOCK,
        "{origin} O_NONBLOCK is not the FreeBSD pin 0x4"
    );
}

#[test]
fn fcntl_open_flags_are_the_freebsd_pinned_values() {
    let installed = read("libc/ravenlibc/include/bits/abi_fcntl.h");
    let generated = read("libc/ravenlibc/abi/generated/include/bits/abi_fcntl.h");
    let table = read("libc/ravenlibc/abi/fcntl.toml");
    let rust = read("libc/ravenlibc/crates/raven-libc-abi/src/generated/fcntl.rs");

    assert_c_header_flags("installed C header", &installed);
    assert_c_header_flags("generated C header", &generated);
    assert_eq!(
        toml_value(&table, "O_CLOEXEC"),
        FCNTL_O_CLOEXEC,
        "abi/fcntl.toml O_CLOEXEC is not the FreeBSD pin 0x100000"
    );
    assert_eq!(
        toml_value(&table, "O_NONBLOCK"),
        FCNTL_O_NONBLOCK,
        "abi/fcntl.toml O_NONBLOCK is not the FreeBSD pin 0x4"
    );
    assert_eq!(
        rust_const(&rust, "O_CLOEXEC"),
        FCNTL_O_CLOEXEC,
        "generated Rust O_CLOEXEC is not the FreeBSD pin 0x100000"
    );
    assert_eq!(
        rust_const(&rust, "O_NONBLOCK"),
        FCNTL_O_NONBLOCK,
        "generated Rust O_NONBLOCK is not the FreeBSD pin 0x4"
    );
}

/// S746 review negative double: the old musl values must fail the same assertion.
///
/// The double is a test-only Rust string, not a C/C++ scaffold file. Compiled only
/// for the explicit review run (`--features s746-negative`). The normal gate keeps
/// the production header untouched and this test absent, so no product code is
/// edited or restored for the negative check.
#[cfg(feature = "s746-negative")]
#[test]
fn fcntl_musl_fixture_is_rejected_by_the_pinned_value_assertion() {
    let fixture = "#define O_CLOEXEC 02000000\n#define O_NONBLOCK 04000\n";
    assert_c_header_flags("isolated musl fixture", fixture);
}
