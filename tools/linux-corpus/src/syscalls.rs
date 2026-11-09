// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Built-in x86_64 syscall number lookup.

#[path = "x86_64_nr.rs"]
mod x86_64_nr;

/// Look up an x86_64 syscall number by its `strace` name.
pub fn x86_64_nr(name: &str) -> Option<u32> {
    x86_64_nr::lookup(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_numbers_match_linux_x86_64() {
        assert_eq!(x86_64_nr("read"), Some(0));
        assert_eq!(x86_64_nr("write"), Some(1));
        assert_eq!(x86_64_nr("openat"), Some(257));
        assert_eq!(x86_64_nr("clone3"), Some(435));
    }

    #[test]
    fn unknown_names_are_none() {
        assert_eq!(x86_64_nr("syscall_0x1b3"), None);
        assert_eq!(x86_64_nr("not-a-syscall"), None);
    }

    #[test]
    fn table_is_sorted_for_binary_search() {
        for pair in super::x86_64_nr::X86_64_NR.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{:?} >= {:?}", pair[0], pair[1]);
        }
    }
}
