// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! T6: no compiler header is shadowed; compiler headers and float values work.

use std::path::Path;

use raven_libc_hdrcheck::clang::{self, Arch, Lang, Vis};
use raven_libc_hdrcheck::{manifest, matrix, repo_root};

const COMPILER_HEADERS: &[&str] = &[
    "stddef.h",
    "stdarg.h",
    "stdbool.h",
    "stdalign.h",
    "stdnoreturn.h",
    "iso646.h",
    "stdatomic.h",
    "stdckdint.h",
    "tgmath.h",
    "varargs.h",
    "unwind.h",
    "float.h",
];

#[test]
fn no_compiler_header_is_shipped() {
    let root = repo_root().join("libc/ravenlibc");
    for directory in [
        root.join("include"),
        root.join("arch/x86_64/include"),
        root.join("arch/aarch64/include"),
        root.join("arch/riscv64/include"),
    ] {
        for name in COMPILER_HEADERS {
            assert!(
                !directory.join(name).exists(),
                "compiler header {name} must not be shipped at {}",
                directory.display()
            );
        }
    }
    for name in COMPILER_HEADERS {
        let rows = manifest::load().expect("manifest");
        if let Some(row) = manifest::find(&rows, name) {
            assert_eq!(row.status, manifest::Status::Absent, "{name}");
        }
    }
}

#[test]
fn stdatomic_compiles_without_libatomic() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let source = "\
#include <stdatomic.h>
struct big { char bytes[16]; };
_Atomic struct big value;
int main(void) {
    atomic_int counter = 0;
    atomic_fetch_add(&counter, 1);
    return (int)atomic_load(&counter);
}
";
    for lang in [Lang::C11, Lang::C17, Lang::C23] {
        let outcome = clang::compile(
            &toolchain,
            &clang::Tu {
                arch: Arch::X86_64,
                lang,
                vis: Vis::Default,
                source: source.to_string(),
            },
        );
        assert!(outcome.ok, "{}: {}", lang.name(), outcome.stderr);
    }
}

#[test]
fn freestanding_compiler_headers_resolve_without_our_dirs() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let source = "\
#include <stddef.h>
#include <stdarg.h>
#include <stdbool.h>
#include <float.h>
int main(void) { return 0; }
";
    for arch in Arch::ALL {
        let outcome = clang::compile_custom(&toolchain, arch, Lang::C11, false, true, &[], source);
        assert!(outcome.ok, "{arch}: {}", outcome.stderr);
    }
}

#[test]
fn limits_follow_hosted_and_freestanding_rules() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let hosted = "#include <limits.h>\n#ifndef PATH_MAX\n#error PATH_MAX must be visible when hosted\n#endif\n#ifndef INT_MAX\n#error INT_MAX\n#endif\nint main(void){return 0;}\n";
    let freestanding = "#include <limits.h>\n#if defined(PATH_MAX)\n#error PATH_MAX must not be visible when freestanding\n#endif\n#ifndef INT_MAX\n#error INT_MAX\n#endif\nint main(void){return 0;}\n";
    for arch in Arch::ALL {
        let outcome =
            clang::compile_custom(&toolchain, arch, Lang::Gnu17, true, false, &[], hosted);
        assert!(outcome.ok, "hosted {arch}: {}", outcome.stderr);
        let outcome =
            clang::compile_custom(&toolchain, arch, Lang::Gnu17, true, true, &[], freestanding);
        assert!(outcome.ok, "freestanding {arch}: {}", outcome.stderr);
    }
}

#[test]
fn stdint_and_inttypes_are_ours_when_hosted() {
    let toolchain = clang::locate_toolchain().expect("clang");
    let source = "#include <stdint.h>\n#include <inttypes.h>\n_Static_assert(sizeof(PRId64) == 3, \"64-bit printf uses l\");\n_Static_assert(sizeof(intmax_t) == 8, \"intmax_t\");\nint main(void){return 0;}\n";
    for arch in Arch::ALL {
        let outcome = clang::compile(
            &toolchain,
            &clang::Tu {
                arch,
                lang: Lang::C11,
                vis: Vis::Default,
                source: source.to_string(),
            },
        );
        assert!(outcome.ok, "{arch}: {}", outcome.stderr);
    }
}

#[test]
fn max_align_and_float_values_are_the_measured_ones() {
    let toolchain = clang::locate_toolchain().expect("clang");
    for (arch, mant) in [
        (Arch::X86_64, 64),
        (Arch::Aarch64, 113),
        (Arch::Riscv64, 113),
    ] {
        let source = format!(
            "\
#include <stddef.h>
#include <float.h>
_Static_assert(sizeof(max_align_t) == 32, \"max_align_t size\");
_Static_assert(_Alignof(max_align_t) == 16, \"max_align_t align\");
_Static_assert(FLT_EVAL_METHOD == 0, \"FLT_EVAL_METHOD\");
_Static_assert(DBL_MANT_DIG == 53, \"DBL_MANT_DIG\");
_Static_assert(FLT_RADIX == 2, \"FLT_RADIX\");
_Static_assert(LDBL_MANT_DIG == {mant}, \"LDBL_MANT_DIG\");
_Static_assert(LDBL_MAX_EXP == 16384, \"LDBL_MAX_EXP\");
_Static_assert(sizeof(long double) == 16, \"long double size\");
int main(void) {{ return 0; }}
"
        );
        let outcome = clang::compile(
            &toolchain,
            &clang::Tu {
                arch,
                lang: Lang::C11,
                vis: Vis::Default,
                source,
            },
        );
        assert!(outcome.ok, "{arch}: {}", outcome.stderr);
        let _ = matrix::MAIN_TAIL;
        let _ = Path::new("");
    }
}
