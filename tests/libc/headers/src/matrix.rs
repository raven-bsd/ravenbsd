// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Matrix construction and probe helpers shared by the eight integration tests.

use crate::clang::{self, Arch, Lang, Outcome, Toolchain, Tu, Vis};

/// The `int main` tail that turns an include-only TU into a valid C or C++
/// translation unit (`-pedantic -Werror` rejects an empty one).
pub const MAIN_TAIL: &str = "int main(void) { return 0; }\n";

/// Build the standard standalone TU for one header.
#[must_use]
pub fn standalone_source(header: &str) -> String {
    format!("#include <{header}>\n{MAIN_TAIL}")
}

/// All T1 translation units for the given headers.
#[must_use]
pub fn t1_tus(headers: &[String], quick: bool) -> Vec<Tu> {
    let arches: Vec<Arch> = if quick {
        vec![Arch::X86_64]
    } else {
        Arch::ALL.to_vec()
    };
    let langs: Vec<Lang> = if quick {
        Lang::QUICK.to_vec()
    } else {
        Lang::ALL.to_vec()
    };
    let visibilities: Vec<Vis> = if quick {
        Vis::QUICK.to_vec()
    } else {
        Vis::ALL.to_vec()
    };
    let mut tus = Vec::new();
    for header in headers {
        let source = standalone_source(header);
        for arch in &arches {
            for lang in &langs {
                for vis in &visibilities {
                    tus.push(Tu {
                        arch: *arch,
                        lang: *lang,
                        vis: *vis,
                        source: source.clone(),
                    });
                }
            }
        }
    }
    tus
}

/// Compile one visibility probe.
pub fn probe(
    toolchain: &Toolchain,
    arch: Arch,
    lang: Lang,
    vis: Vis,
    header: &str,
    symbol: &str,
) -> Outcome {
    let pragma = if symbol == "gets" {
        "#pragma clang diagnostic ignored \"-Wdeprecated-declarations\"\n"
    } else {
        ""
    };
    let source = format!(
        "#include <{header}>\n{pragma}void hdrcheck_probe(void) {{ (void)({symbol}); }}\n{MAIN_TAIL}"
    );
    clang::compile(
        toolchain,
        &Tu {
            arch,
            lang,
            vis,
            source,
        },
    )
}

/// The mode-name table used by `probes.toml`.
#[must_use]
pub fn mode(name: &str) -> Option<(Lang, Vis)> {
    Some(match name {
        "default" => (Lang::Gnu17, Vis::Default),
        "posix" => (Lang::Gnu17, Vis::PosixStrict),
        "xopen" => (Lang::Gnu17, Vis::XopenStrict),
        "gnu" => (Lang::Gnu17, Vis::Gnu),
        "ansi" => (Lang::Gnu17, Vis::Ansi),
        "c11" => (Lang::C11, Vis::Default),
        "c23" => (Lang::C23, Vis::Default),
        "cxx17" => (Lang::Cxx17, Vis::Default),
        "cxx20" => (Lang::Cxx20, Vis::Default),
        _ => return None,
    })
}

/// Mode names covered by the feature table of T5.
pub const FEATURE_MODES: &[&str] = &[
    "default", "posix", "xopen", "gnu", "c11", "c23", "ansi", "cxx17", "cxx20",
];

/// Parse the five visibility macros out of a `-dM` dump.
#[must_use]
pub fn visibility_values(dump: &[(String, String)]) -> Option<[String; 5]> {
    let names = [
        "__POSIX_VISIBLE",
        "__XSI_VISIBLE",
        "__BSD_VISIBLE",
        "__ISO_C_VISIBLE",
        "__EXT1_VISIBLE",
    ];
    let mut values = Vec::new();
    for name in names {
        values.push(dump.iter().find(|(key, _)| key == name)?.1.clone());
    }
    values.try_into().ok()
}

/// Compile a symbol-export TU with `hdr_t1` style arguments.
#[must_use]
pub fn probe_args(
    toolchain: &Toolchain,
    arch: Arch,
    lang: Lang,
    vis: Vis,
    header: &str,
    symbol: &str,
) -> Vec<String> {
    clang::clang_args(
        toolchain,
        &Tu {
            arch,
            lang,
            vis,
            source: format!("#include <{header}>\n(void)({symbol});\n"),
        },
    )
}

#[cfg(test)]
#[path = "matrix_ut.rs"]
mod tests;
