// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `.note.raven.abi` note, once.
//!
//! CLEANROOM from `donor-map/14` 3.A.8: section name `.note.raven.abi`, owner `"RavenBSD"`,
//! type 1, descriptor `u32` ABI revision 1. `EI_OSABI` stays 0. The older
//! `.note.raven.ident` name of `donor-map/10` 3.12.4 is superseded (C61 RD-new).
//!
//! The three entry objects (`crt1`, `Scrt1`, `rcrt1`) expand the macro inside their single
//! `global_asm!`; `concat!` keeps the text in one place. The macro form is required because
//! `global_asm!` accepts a string literal (or `concat!`) only, never a `const` path.

/// The `.note.raven.abi` assembly text.
#[allow(unused_macros)] // host builds compile the assembly modules out
macro_rules! note_raven_abi {
    () => {
        concat!(
            ".section .note.raven.abi,\"a\",%note\n",
            ".balign 4\n",
            ".long 9\n", // namesz: sizeof("RavenBSD\0")
            ".long 4\n", // descsz: one u32
            ".long 1\n", // type: NT_VERSION
            ".asciz \"RavenBSD\"\n",
            ".balign 4\n",
            ".long 1\n", // descriptor: ABI revision 1
            ".balign 4\n",
        )
    };
}
