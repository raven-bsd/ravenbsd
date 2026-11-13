// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T2: include order. Each header twice, each header first, and 2,000 sampled
//! pairs in both orders (doc 14 3.B.4; xorshift64 seed 0x5241564E).

use raven_libc_hdrcheck::clang::{self, Arch, Lang, Tu, Vis};
use raven_libc_hdrcheck::{manifest, matrix};

const SEED: u64 = 0x5241_564E;

fn xorshift64(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *state = value;
    value
}

fn wrap(source: String, lang: Lang) -> String {
    if lang.is_cxx() {
        format!("{source}int main(void) {{ return 0; }}\n")
    } else {
        format!("{source}{}", matrix::MAIN_TAIL)
    }
}

#[test]
fn include_order_is_stable() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let rows = manifest::load().expect("manifest");
    let headers = manifest::owned_standalone(&rows);
    let quick = manifest::quick();
    let samples = if quick { 100 } else { 2_000 };
    let mut tus = Vec::new();

    // Every header twice in one TU.
    for arch in Arch::ALL {
        for lang in [Lang::Gnu17, Lang::Cxx17] {
            for header in &headers {
                let source = wrap(format!("#include <{header}>\n#include <{header}>\n"), lang);
                tus.push(Tu {
                    arch,
                    lang,
                    vis: Vis::Default,
                    source,
                });
            }
        }
    }

    // One big TU per arch, manifest order then reverse, in gnu17 and c++17.
    let mut forward = String::new();
    let mut reverse = String::new();
    for header in &headers {
        forward.push_str(&format!("#include <{header}>\n"));
    }
    for header in headers.iter().rev() {
        reverse.push_str(&format!("#include <{header}>\n"));
    }
    for arch in Arch::ALL {
        for lang in [Lang::Gnu17, Lang::Cxx17] {
            for source in [&forward, &reverse] {
                tus.push(Tu {
                    arch,
                    lang,
                    vis: Vis::Default,
                    source: wrap(source.clone(), lang),
                });
            }
        }
    }

    // Every header first, then every other header.
    for arch in Arch::ALL {
        for header in &headers {
            let mut source = format!("#include <{header}>\n");
            for other in &headers {
                if other != header {
                    source.push_str(&format!("#include <{other}>\n"));
                }
            }
            tus.push(Tu {
                arch,
                lang: Lang::Gnu17,
                vis: Vis::Default,
                source: wrap(source, Lang::Gnu17),
            });
        }
    }

    // Sampled pairs in both orders.
    let mut state = SEED;
    for _ in 0..samples {
        let first = (xorshift64(&mut state) as usize) % headers.len();
        let second = (xorshift64(&mut state) as usize) % headers.len();
        let left = &headers[first];
        let right = &headers[second];
        for source in [
            format!("#include <{left}>\n#include <{right}>\n"),
            format!("#include <{right}>\n#include <{left}>\n"),
        ] {
            tus.push(Tu {
                arch: Arch::X86_64,
                lang: Lang::Gnu17,
                vis: Vis::Default,
                source: wrap(source, Lang::Gnu17),
            });
        }
    }

    println!("T2: {} TUs ({} sampled pairs)", tus.len(), samples);
    let results = clang::run_matrix(&toolchain, tus, manifest::jobs());
    let mut failures = 0;
    for (tu, outcome) in &results {
        if !outcome.ok {
            failures += 1;
            if failures <= 20 {
                eprintln!(
                    "FAIL {} {} {}\n{}\n--- source ---\n{}",
                    tu.arch,
                    tu.lang.name(),
                    tu.vis.name(),
                    outcome.stderr,
                    tu.source
                );
            }
        }
    }
    println!(
        "T2: {} passed, {} failed",
        results.len() - failures,
        failures
    );
    assert_eq!(failures, 0, "T2 failures: {failures}");
}
