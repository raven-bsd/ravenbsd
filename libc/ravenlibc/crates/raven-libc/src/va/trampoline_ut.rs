// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the trampoline macro (`trampoline.rs`): the macro must assemble on the host
//! and pass the named arguments plus the `va_list` to the target unchanged.
//!
//! The real calling-convention differential lives in `tests/libc/va` (the host test crate);
//! this file only proves that the macro expands, that the symbol is defined and that the
//! generated stub calls the target with the expected `va_list`.

#[cfg(target_arch = "x86_64")]
mod probe {
    use crate::va::VaReaderExt;
    use crate::va::sysv_x86_64::{SysvReader, VaListSysV};

    /// The `v*` target of the probe stub: `sum` of the three named words plus the first
    /// integer argument of the list.
    pub extern "C" fn target(a: usize, b: usize, c: usize, ap: *mut VaListSysV) -> i32 {
        // SAFETY: the stub built the list in its own frame and passes a pointer to it.
        let mut reader = unsafe { SysvReader::from_ptr(ap) };
        let variadic = reader.next_i32();
        let sum = a.wrapping_add(b).wrapping_add(c);
        // SAFETY: `sum` is a small bounded counter here.
        i32::try_from(sum).unwrap_or(0).wrapping_add(variadic)
    }
}

#[cfg(target_arch = "x86_64")]
crate::raven_variadic!(prefix: "t_", export: "probe", target: probe::target, named: 3);

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    /// The stub the macro above defines.
    fn t_probe(a: usize, b: usize, c: usize, ...) -> i32;
}

#[test]
#[cfg(target_arch = "x86_64")]
fn the_stub_passes_named_arguments_and_builds_a_list() {
    // SAFETY: the format-free probe target expects three words and one integer argument,
    // which is exactly what this call passes.
    let result = unsafe { t_probe(1, 2, 3, 42i32) };
    assert_eq!(result, 6 + 42);
}

#[test]
fn the_named_bound_is_what_the_abi_allows() {
    assert_eq!(crate::va::MAX_NAMED_X86_64, 5);
    assert_eq!(crate::va::MAX_NAMED_OTHER, 7);
}
