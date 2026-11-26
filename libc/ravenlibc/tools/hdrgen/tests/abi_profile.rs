// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Rust probe for the compile-only FreeBSD application-profile fixture (S721).
//!
//! The probe emits the C source into a temporary directory at run time, compiles it with
//! clang against a donor-shaped include tree (positive control) and against Raven's staged
//! public header root, then names the baseline profile rows. No C source or header is
//! committed to the product tree; the structural policy forbids that outside `addons/`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

/// The compile-only fixture: every assertion is a public FreeBSD application-profile fact.
const FIXTURE: &str = r#"/*
 * SPDX-License-Identifier: BSD-3-Clause
 * Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
 *
 * Compile-only FreeBSD application-profile fixture (DR-69, S721).
 *
 * Expected values come from the pinned FreeBSD donor
 * 133344405f532bd54ff3629a77d4c4e062f01fdb:
 *   sys/sys/errno.h        EAGAIN 35
 *   sys/sys/signal.h       SIGUSR1 30
 *   sys/sys/_types.h       typedef __uint8_t sa_family_t
 *   sys/sys/socket.h       struct sockaddr_in, sin_family at offset 1
 */

#include <errno.h>
#include <signal.h>
#include <stddef.h>
#include <sys/socket.h>
#include <netinet/in.h>

_Static_assert(EAGAIN == 35, "FreeBSD errno: EAGAIN is 35");
_Static_assert(sizeof(sa_family_t) == 1, "FreeBSD sa_family_t is one byte");
_Static_assert(
    offsetof(struct sockaddr_in, sin_family) == 1,
    "FreeBSD sockaddr_in.sin_family is at offset 1 after sin_len");
_Static_assert(sizeof(struct sockaddr_in) == 16, "FreeBSD sockaddr_in is 16 bytes");
_Static_assert(sizeof(struct sockaddr) == 16, "FreeBSD sockaddr is 16 bytes");
_Static_assert(
    offsetof(struct sockaddr, sa_len) == 0,
    "FreeBSD sockaddr.sa_len is the first byte");
_Static_assert(
    offsetof(struct sockaddr, sa_family) == 1,
    "FreeBSD sockaddr.sa_family follows sa_len");
"#;

/// Baseline rows the staged Raven headers still report as red after the FreeBSD cutovers.
///
/// The errno row (`EAGAIN` 35) is satisfied by S722 and the signal row (`SIGUSR1` 30) by S713;
/// both are asserted separately as green.
const BASELINE_ROWS: [&str; 4] = [
    "FreeBSD sa_family_t is one byte",
    "FreeBSD sockaddr_in.sin_family is at offset 1 after sin_len",
    "no member named 'sa_len'",
    "FreeBSD sockaddr.sa_family follows sa_len",
];

/// The row S722 makes true in the staged Raven headers.
const SATISFIED_ERRNO_ROW: &str = "FreeBSD errno: EAGAIN is 35";

/// The row S713 makes true in the staged Raven headers.
const SATISFIED_SIGNAL_ROW: &str = "FreeBSD signal: SIGUSR1 is 30";

const SYNTHETIC_ERRNO: &str = "#define EAGAIN 35\n";
const SYNTHETIC_SIGNAL: &str = "#define SIGUSR1 30\n";
const SYNTHETIC_SOCKET: &str = r#"#ifndef PROBE_SYS_SOCKET_H
#define PROBE_SYS_SOCKET_H
typedef unsigned char sa_family_t;
struct sockaddr {
    unsigned char sa_len;
    sa_family_t sa_family;
    char sa_data[14];
};
#endif
"#;
const SYNTHETIC_IN: &str = r#"#ifndef PROBE_NETINET_IN_H
#define PROBE_NETINET_IN_H
#include <sys/socket.h>
typedef unsigned short in_port_t;
struct in_addr {
    unsigned int s_addr;
};
struct sockaddr_in {
    unsigned char sin_len;
    sa_family_t sin_family;
    in_port_t sin_port;
    struct in_addr sin_addr;
    char sin_zero[8];
};
#endif
"#;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "raven-libc-hdrgen-abi-profile-{tag}-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("create probe temp directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn resource_include() -> PathBuf {
    let output = Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .expect("run clang -print-resource-dir");
    assert!(
        output.status.success(),
        "clang -print-resource-dir failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).join("include")
}

fn compile(arch: &str, source: &Path, includes: &[PathBuf]) -> Output {
    let mut command = Command::new("clang");
    command
        .arg(format!("--target={arch}-unknown-ravenbsd"))
        .arg("-nostdinc")
        .arg("-fsyntax-only");
    for include in includes {
        assert!(
            include.is_dir(),
            "include root is missing: {}",
            include.display()
        );
        command.arg("-isystem").arg(include);
    }
    command.arg(source);
    command.output().expect("run clang on the generated probe")
}

fn write_synthetic_donor(root: &Path) {
    std::fs::create_dir_all(root.join("sys")).expect("create synthetic sys directory");
    std::fs::create_dir_all(root.join("netinet")).expect("create synthetic netinet directory");
    std::fs::write(root.join("errno.h"), SYNTHETIC_ERRNO).expect("write synthetic errno.h");
    std::fs::write(root.join("signal.h"), SYNTHETIC_SIGNAL).expect("write synthetic signal.h");
    std::fs::write(root.join("sys/socket.h"), SYNTHETIC_SOCKET)
        .expect("write synthetic sys/socket.h");
    std::fs::write(root.join("netinet/in.h"), SYNTHETIC_IN).expect("write synthetic netinet/in.h");
}

fn collect_c_paths(directory: &Path, found: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_c_paths(&path, found);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" | "hxx"
                )
            })
        {
            found.push(path);
        }
    }
}

#[test]
fn no_c_or_cpp_source_is_committed_in_the_abi_profile_stage() {
    let stage = repo_root().join("tests/conformance/abi-profile");
    assert!(stage.is_dir(), "missing stage {}", stage.display());
    let mut found = Vec::new();
    collect_c_paths(&stage, &mut found);
    assert!(
        found.is_empty(),
        "C/C++ source is forbidden in the product tree outside addons: {found:?}"
    );
}

#[test]
fn the_generated_probe_compiles_and_names_every_baseline_row() {
    let root = repo_root();
    let temp = TempDir::new("probe");
    let source = temp.path().join("fixture.c");
    std::fs::write(&source, FIXTURE).expect("write the generated probe");
    let resource = resource_include();

    for arch in ["x86_64", "aarch64"] {
        let synthetic = temp.path().join(format!("donor-shape-{arch}"));
        write_synthetic_donor(&synthetic);
        let donor = compile(arch, &source, &[resource.clone(), synthetic]);
        assert!(
            donor.status.success(),
            "donor-shaped probe for {arch} must compile: {}",
            String::from_utf8_lossy(&donor.stderr)
        );

        let includes = [
            resource.clone(),
            root.join("libc/ravenlibc/include"),
            root.join("libc/ravenlibc/abi/generated/include"),
            root.join(format!("libc/ravenlibc/arch/{arch}/include")),
        ];
        let raven = compile(arch, &source, &includes);
        let text = String::from_utf8_lossy(&raven.stderr).into_owned();
        assert!(
            !raven.status.success(),
            "Raven headers for {arch} unexpectedly compiled the FreeBSD profile probe"
        );
        for row in BASELINE_ROWS {
            assert!(
                text.contains(row),
                "{arch} probe did not name baseline row {row:?}:\n{text}"
            );
        }
        assert!(
            !text.contains(SATISFIED_ERRNO_ROW),
            "{arch} Raven headers still reject the satisfied errno row {SATISFIED_ERRNO_ROW:?}:\n{text}"
        );
        assert!(
            !text.contains(SATISFIED_SIGNAL_ROW),
            "{arch} Raven headers still reject the satisfied signal row {SATISFIED_SIGNAL_ROW:?}:\n{text}"
        );
    }
}
