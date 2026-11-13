// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T5: the feature-test engine (the ten-row table of ticket 4.1) and the
//! symbol-visibility probes.

use raven_libc_hdrcheck::clang::{self, Arch, Lang};
use raven_libc_hdrcheck::{consts, manifest, matrix};

#[test]
fn visibility_table_is_exact() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows: [(&str, Lang, &[&str], [&str; 5]); 9] = [
        (
            "default",
            Lang::Gnu17,
            &[],
            ["202405", "800", "1", "2023", "1"],
        ),
        (
            "posix",
            Lang::Gnu17,
            &["-D_POSIX_C_SOURCE=200809L"],
            ["200809", "0", "0", "2011", "0"],
        ),
        (
            "xopen",
            Lang::Gnu17,
            &["-D_XOPEN_SOURCE=700"],
            ["200809", "700", "0", "2011", "0"],
        ),
        (
            "gnu",
            Lang::Gnu17,
            &["-D_GNU_SOURCE", "-D_POSIX_C_SOURCE=200809L"],
            ["202405", "800", "1", "2023", "1"],
        ),
        ("c11", Lang::C11, &[], ["0", "0", "0", "2011", "0"]),
        ("c23", Lang::C23, &[], ["0", "0", "0", "2023", "0"]),
        (
            "ansi",
            Lang::Gnu17,
            &["-D_ANSI_SOURCE"],
            ["0", "0", "0", "1990", "0"],
        ),
        (
            "cxx17",
            Lang::Cxx17,
            &[],
            ["202405", "800", "1", "2023", "1"],
        ),
        (
            "cxx20",
            Lang::Cxx20,
            &[],
            ["202405", "800", "1", "2023", "1"],
        ),
    ];
    for (name, lang, defines, expected) in rows {
        let dump = clang::macro_dump(
            &toolchain,
            Arch::X86_64,
            lang,
            "#include <sys/_visible.h>\n",
            defines,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let values =
            matrix::visibility_values(&dump).unwrap_or_else(|| panic!("{name}: missing macro"));
        assert_eq!(
            [
                values[0].as_str(),
                values[1].as_str(),
                values[2].as_str(),
                values[3].as_str(),
                values[4].as_str()
            ],
            expected,
            "row {name}"
        );
        println!("T5 row {name}: {}", values.join(" "));
    }

    // _FILE_OFFSET_BITS=64 and _TIME_BITS=64 are the default environment.
    let dump = clang::macro_dump(
        &toolchain,
        Arch::X86_64,
        Lang::Gnu17,
        "#include <sys/_visible.h>\n",
        &["-D_FILE_OFFSET_BITS=64", "-D_TIME_BITS=64"],
    )
    .expect("64-bit knobs accepted");
    let values = matrix::visibility_values(&dump).expect("five values");
    assert_eq!(values, ["202405", "800", "1", "2023", "1"]);

    // 32 is a hard error with the documented message.
    for define in ["-D_FILE_OFFSET_BITS=32", "-D_TIME_BITS=32"] {
        let error = clang::macro_dump(
            &toolchain,
            Arch::X86_64,
            Lang::Gnu17,
            "#include <sys/_visible.h>\n",
            &[define],
        )
        .expect_err("32-bit knob must fail");
        assert!(
            error.contains("must be 64 or undefined"),
            "{define}: {error}"
        );
        println!(
            "T5 error row {define}: {}",
            error.lines().next().unwrap_or("")
        );
    }
}

#[test]
fn symbol_probes_match_the_table() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows = consts::load_probes().expect("probes.toml");
    assert!(rows.len() >= 25, "only {} probes", rows.len());
    let required = manifest::required_owners();
    let complete = manifest::require_complete();
    let mut failures = 0;
    for row in &rows {
        if row.pending {
            let owner = &row.owner;
            if complete || required.iter().any(|item| item == owner) {
                failures += 1;
                eprintln!(
                    "PENDING probe {}:{} requires owner {owner}",
                    row.header, row.symbol
                );
            } else {
                println!(
                    "PENDING probe {}:{} (owner {owner})",
                    row.header, row.symbol
                );
            }
            continue;
        }
        for mode_name in &row.visible_in {
            let (lang, vis) = matrix::mode(mode_name).expect("mode");
            let outcome = matrix::probe(
                &toolchain,
                Arch::X86_64,
                lang,
                vis,
                &row.header,
                &row.symbol,
            );
            if !outcome.ok {
                failures += 1;
                eprintln!(
                    "MUST BE VISIBLE {}:{} {mode_name}\n{}",
                    row.header, row.symbol, outcome.stderr
                );
            }
        }
        for mode_name in &row.hidden_in {
            let (lang, vis) = matrix::mode(mode_name).expect("mode");
            let outcome = matrix::probe(
                &toolchain,
                Arch::X86_64,
                lang,
                vis,
                &row.header,
                &row.symbol,
            );
            if outcome.ok {
                failures += 1;
                eprintln!("MUST BE HIDDEN {}:{} {mode_name}", row.header, row.symbol);
            }
        }
        println!(
            "T5 probe {}:{} visible in {:?}",
            row.header, row.symbol, row.visible_in
        );
    }
    assert_eq!(failures, 0, "T5 probe failures: {failures}");
}
