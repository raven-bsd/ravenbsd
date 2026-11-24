// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The aarch64 sendsig frame checked against the S160 donor table.
//!
//! The table is `tests/abi/signal-frame/donor-layout.txt`. It currently pins the amd64 layout, so
//! the shared fields are expected to agree up to the point where the aarch64 machine context
//! changes the size: the first mismatch names the offset that the table still lacks for aarch64.

use super::*;

fn parse_hex(text: &str) -> usize {
    usize::from_str_radix(text.trim_start_matches("0x"), 16).expect("hex value")
}

fn donor_table() -> std::collections::HashMap<String, usize> {
    let mut found = None;
    for dir in std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
        let candidate = dir.join("tests/abi/signal-frame/donor-layout.txt");
        if candidate.is_file() {
            found = Some(candidate);
            break;
        }
    }
    let path = found.expect("tests/abi/signal-frame/donor-layout.txt");
    let text = std::fs::read_to_string(path).expect("donor layout");
    let mut rows = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("sizeof(") {
            let (name, rest) = rest.split_once(")=").expect("sizeof row");
            let (size, _align) = rest.split_once(" align=").expect("sizeof align");
            rows.insert(format!("sizeof({name})"), parse_hex(size));
        } else if let Some((name, value)) = line.split_once('=') {
            rows.insert(name.to_string(), parse_hex(value));
        }
    }
    rows
}

#[test]
fn aarch64_frame_matches_the_s160_donor_table() {
    let table = donor_table();
    let value = |name: &str| {
        *table
            .get(name)
            .unwrap_or_else(|| panic!("{name} missing from the S160 donor table"))
    };

    assert_eq!(
        SIGINFO_SIZE,
        value("sizeof(fbsd_siginfo_t)"),
        "sizeof(fbsd_siginfo_t): S160 amd64 vs aarch64"
    );
    assert_eq!(
        UC_SIGMASK_OFFSET,
        value("fbsd_ucontext_t.uc_sigmask"),
        "fbsd_ucontext_t.uc_sigmask"
    );
    assert_eq!(
        UC_MCONTEXT_OFFSET,
        value("fbsd_ucontext_t.uc_mcontext"),
        "fbsd_ucontext_t.uc_mcontext"
    );
    assert_eq!(
        UC_LINK_OFFSET,
        value("fbsd_ucontext_t.uc_link"),
        "fbsd_ucontext_t.uc_link: S160 amd64 vs aarch64"
    );
    assert_eq!(
        UC_STACK_OFFSET,
        value("fbsd_ucontext_t.uc_stack"),
        "fbsd_ucontext_t.uc_stack: S160 amd64 vs aarch64"
    );
    assert_eq!(
        UC_FLAGS_OFFSET,
        value("fbsd_ucontext_t.uc_flags"),
        "fbsd_ucontext_t.uc_flags: S160 amd64 vs aarch64"
    );
    assert_eq!(
        UCONTEXT_SIZE,
        value("sizeof(fbsd_ucontext_t)"),
        "sizeof(fbsd_ucontext_t): S160 amd64 vs aarch64"
    );
    assert_eq!(
        MC_GREGS_OFFSET,
        value("nb_mcontext_t.__gregs"),
        "nb_mcontext_t.__gregs"
    );
    assert_eq!(
        MC_FREGS_OFFSET,
        value("nb_mcontext_t.__fpregs"),
        "nb_mcontext_t.__fpregs: S160 amd64 vs aarch64 __fregs"
    );
    assert_eq!(
        MCONTEXT_SIZE,
        value("sizeof(nb_mcontext_t)"),
        "sizeof(nb_mcontext_t): S160 amd64 vs aarch64"
    );
}
