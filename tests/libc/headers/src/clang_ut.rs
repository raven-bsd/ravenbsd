// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the clang runner. These are the negative controls of
//! ticket section 7.1: the harness must return a failing `Outcome`.

use super::*;

fn toolchain() -> Toolchain {
    locate_toolchain().expect("clang is installed and on PATH (or RAVEN_CLANG)")
}

#[test]
fn locates_clang_and_a_triple_flavour() {
    let toolchain = toolchain();
    assert!(toolchain.resource_include.is_dir());
    assert!(!toolchain.version.is_empty());
    assert!(matches!(
        toolchain.triple_flavour,
        TripleFlavour::Ravenbsd | TripleFlavour::NoneElf
    ));
}

#[test]
fn argument_vector_is_exact() {
    let toolchain = toolchain();
    let tu = Tu {
        arch: Arch::X86_64,
        lang: Lang::Gnu17,
        vis: Vis::PosixStrict,
        source: String::new(),
    };
    let args = clang_args(&toolchain, &tu);
    assert!(args[0].starts_with("--target=x86_64-unknown-"));
    assert!(args.contains(&"-nostdinc".to_string()));
    assert!(args.contains(&"-D__ravenbsd__=1".to_string()));
    assert!(args.contains(&"-D_POSIX_C_SOURCE=200809L".to_string()));
    assert_eq!(args.last().map(String::as_str), Some("-"));
    assert!(args.iter().any(|arg| arg == "-std=gnu17"));
    let cxx = Tu {
        arch: Arch::Aarch64,
        lang: Lang::Cxx20,
        vis: Vis::Default,
        source: String::new(),
    };
    let args = clang_args(&toolchain, &cxx);
    assert!(args.iter().any(|arg| arg == "-std=c++20"));
    assert!(args.iter().any(|arg| arg == "-nostdinc++"));
}

#[test]
fn c23_falls_back_to_c2x_when_rejected() {
    let toolchain = toolchain();
    let tu = Tu {
        arch: Arch::X86_64,
        lang: Lang::C23,
        vis: Vis::Default,
        source: String::new(),
    };
    let args = clang_args(&toolchain, &tu);
    assert!(
        args.iter()
            .any(|arg| arg == "-std=c23" || arg == "-std=c2x"),
        "{args:?}"
    );
}

#[test]
fn compiles_a_good_tu() {
    let toolchain = toolchain();
    let outcome = compile(
        &toolchain,
        &Tu {
            arch: Arch::X86_64,
            lang: Lang::Gnu17,
            vis: Vis::Default,
            source: "#include <stdint.h>\nint main(void) { return 0; }\n".to_string(),
        },
    );
    assert!(outcome.ok, "{}", outcome.stderr);
}

#[test]
fn deliberately_bad_headers_fail() {
    let toolchain = toolchain();
    let missing = compile(
        &toolchain,
        &Tu {
            arch: Arch::X86_64,
            lang: Lang::Gnu17,
            vis: Vis::Default,
            source: "#include <nonexistent.h>\nint main(void) { return 0; }\n".to_string(),
        },
    );
    assert!(!missing.ok);
    assert!(
        missing.stderr.contains("nonexistent.h"),
        "{}",
        missing.stderr
    );
    let conflict = compile(
        &toolchain,
        &Tu {
            arch: Arch::X86_64,
            lang: Lang::Gnu17,
            vis: Vis::Default,
            source: "typedef int x;\ntypedef long x;\nint main(void) { return 0; }\n".to_string(),
        },
    );
    assert!(!conflict.ok);
}

#[test]
fn macro_dump_reads_visibility_values() {
    let toolchain = toolchain();
    let dump = macro_dump(
        &toolchain,
        Arch::X86_64,
        Lang::Gnu17,
        "#include <sys/_visible.h>\n",
        &[],
    )
    .expect("dump");
    let values = crate::matrix::visibility_values(&dump).expect("five values");
    assert_eq!(values, ["202405", "800", "1", "2023", "1"]);
}

#[test]
fn run_matrix_keeps_input_order() {
    let toolchain = toolchain();
    let tus: Vec<Tu> = (0..3)
        .map(|index| Tu {
            arch: Arch::X86_64,
            lang: Lang::Gnu17,
            vis: Vis::Default,
            source: format!("int value_{index};\nint main(void) {{ return 0; }}\n"),
        })
        .collect();
    let results = run_matrix(&toolchain, tus.clone(), 2);
    assert_eq!(results.len(), 3);
    for (index, (tu, outcome)) in results.iter().enumerate() {
        assert_eq!(tu, &tus[index]);
        assert!(outcome.ok, "{}", outcome.stderr);
    }
}
