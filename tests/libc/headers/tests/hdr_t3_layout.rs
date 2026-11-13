// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T3: struct layouts of doc 14 3.A.3/3.A.3a and the mlibc ABI-break list.

use raven_libc_hdrcheck::clang::{self, Arch, Lang, Tu, Vis};
use raven_libc_hdrcheck::{layout, manifest};

#[test]
fn every_owned_layout_row_passes() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows = manifest::load().expect("manifest");
    let layouts = layout::load().expect("layouts.toml");
    assert!(
        layouts.len() >= 75,
        "layouts.toml has {} rows",
        layouts.len()
    );
    let required = manifest::required_owners();
    let complete = manifest::require_complete();

    let mut pending = Vec::new();
    let mut failures = 0;
    for arch in Arch::ALL {
        let source = layout::assert_source(&layouts, arch, &rows);
        let outcome = clang::compile(
            &toolchain,
            &Tu {
                arch,
                lang: Lang::Gnu17,
                vis: Vis::Default,
                source: source.clone(),
            },
        );
        if !outcome.ok {
            failures += 1;
            eprintln!(
                "FAIL layout asserts on {arch}:\n{}\n--- source ---\n{source}",
                outcome.stderr
            );
        }
    }
    for row in &layouts {
        let owners = layout::owner(row, &rows);
        let any_arch = Arch::ALL
            .iter()
            .any(|arch| layout::available(row, *arch, &rows));
        if !any_arch {
            pending.push(format!("{} (owner {owners})", row.name));
            if owners == "C37" || complete || required.iter().any(|owner| owner == &owners) {
                failures += 1;
                eprintln!("PENDING row {} requires its owner {owners}", row.name);
            }
        }
    }
    println!(
        "T3: {} layout rows, {} pending",
        layouts.len(),
        pending.len()
    );
    for line in &pending {
        println!("PENDING layout: {line}");
    }
    assert_eq!(failures, 0, "T3 failures: {failures}");
}

#[test]
fn mlibc_abi_break_negative_list_is_covered() {
    let layouts = layout::load().expect("layouts.toml");
    let row = |name: &str| {
        layouts
            .iter()
            .find(|row| row.name == name)
            .unwrap_or_else(|| panic!("missing layout row {name}"))
    };
    assert_eq!(row("nlink_t").size, 4);
    assert_eq!(row("blksize_t").size, 4);
    assert_eq!(row("struct statvfs").size, 112);
    assert_eq!(
        row("struct dirent")
            .fields
            .iter()
            .find(|field| field.name == "d_name")
            .unwrap()
            .bytes,
        Some(256)
    );
    assert_eq!(row("jmp_buf").size, 392);
    assert_eq!(row("sigset_t").size, 128);
    // ucontext_t/mcontext_t and epoll_event are per arch.
    for arch in Arch::ALL {
        let names: Vec<&str> = layouts
            .iter()
            .filter(|row| {
                row.name == "ucontext_t" && row.arches.iter().any(|name| name == arch.name())
            })
            .map(|row| row.name.as_str())
            .collect();
        assert_eq!(names.len(), 1, "ucontext_t missing for {arch}");
    }
    println!(
        "T3: mlibc negative list (blksize_t, nlink_t, statvfs, d_name, jmp_buf, sigset_t, ucontext_t) covered"
    );
}
