// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C8 process-stack argument view.
//!
//! The kernel (and later `raven_rt::spawn`) builds the stack; this parser trusts its shape but
//! bounds every read: `argc <= 4096`, each string at most 64 KiB, at most 4096 environment entries
//! and at most 64 auxv entries. Malformed entries end iteration instead of panicking.

const MAX_ARGC: usize = 4096;
const MAX_ENV: usize = 4096;
const MAX_AUXV: usize = 64;
const MAX_STRING: usize = 64 * 1024;

/// A view over the process stack built by the kernel.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Args {
    argc: usize,
    argv: *const *const usize,
    envp: *const *const usize,
    env_count: usize,
    auxv: *const usize,
}

impl Args {
    /// An empty argument set (used for malformed or misaligned stacks and unit tests).
    pub const fn empty() -> Self {
        Self {
            argc: 0,
            argv: core::ptr::null(),
            envp: core::ptr::null(),
            env_count: 0,
            auxv: core::ptr::null(),
        }
    }

    /// Number of arguments (including `argv[0]`).
    pub fn len(&self) -> usize {
        self.argc
    }

    /// True when there are no arguments.
    pub fn is_empty(&self) -> bool {
        self.argc == 0
    }

    /// Return argument `index` if it exists and is valid UTF-8.
    pub fn get(&self, index: usize) -> Option<&'static str> {
        if index >= self.argc {
            return None;
        }
        // SAFETY: `parse_stack` validated that the argv array holds `argc + 1` readable words
        // and `index < argc` was checked above.
        let pointer = unsafe { *self.argv.add(index) };
        // SAFETY: every non-null argv entry names a NUL-terminated string from the C8 stack.
        let bytes = unsafe { c_string(pointer) }?;
        core::str::from_utf8(bytes).ok()
    }

    /// Iterate over valid UTF-8 arguments; invalid UTF-8 entries are skipped.
    pub fn iter(&self) -> impl Iterator<Item = &'static str> + '_ {
        ArgsIter {
            args: *self,
            index: 0,
        }
    }

    /// Return the value of environment variable `key`, if present and valid UTF-8.
    pub fn env_get(&self, key: &str) -> Option<&'static str> {
        if self.envp.is_null() {
            return None;
        }
        for index in 0..self.env_count {
            // SAFETY: `parse_stack` counted `env_count` readable, non-null entries followed by
            // the NULL terminator, and `index < env_count`.
            let pointer = unsafe { *self.envp.add(index) };
            // SAFETY: every non-null environment entry names a NUL-terminated C8 string.
            let bytes = unsafe { c_string(pointer) }?;
            let Ok(text) = core::str::from_utf8(bytes) else {
                continue;
            };
            if let Some((name, value)) = text.split_once('=')
                && name == key
            {
                return Some(value);
            }
        }
        None
    }

    /// Return the value of auxv key `key`, if present before the terminating `(0, 0)`.
    pub fn auxv(&self, key: usize) -> Option<usize> {
        if self.auxv.is_null() {
            return None;
        }
        for index in 0..MAX_AUXV {
            // SAFETY: `self.auxv` points at the C8 auxv array, a sequence of (key, value) usize
            // pairs terminated by (0, 0); `index < MAX_AUXV` bounds the scan.
            let pair = unsafe { self.auxv.add(index * 2) };
            // SAFETY: each pair is two readable words within the auxv array.
            let entry_key = unsafe { *pair };
            // SAFETY: each pair is two readable words within the auxv array.
            let value = unsafe { *pair.add(1) };
            if entry_key == 0 {
                return None;
            }
            if entry_key == key {
                return Some(value);
            }
        }
        None
    }
}

/// Parse the C8 stack at `sp0`.
///
/// # Safety
///
/// `sp0` must point to the process stack the kernel or `spawn` built, or be null/misaligned.
pub(crate) unsafe fn parse_stack(sp0: *const usize) -> Args {
    let address = sp0 as usize;
    if sp0.is_null() || !address.is_multiple_of(core::mem::align_of::<usize>()) {
        return Args::empty();
    }
    // SAFETY: the caller guarantees `sp0` is the C8 stack pointer; it is non-null and aligned.
    let argc = unsafe { *sp0 };
    if argc > MAX_ARGC {
        return Args::empty();
    }
    // SAFETY: the C8 layout places the argv pointer array immediately after `argc`.
    let argv = unsafe { sp0.add(1) } as *const *const usize;
    // The layout guarantees argv[argc] == NULL; reject a stack that does not.
    // SAFETY: the C8 stack has `argc` argv words followed by the NULL terminator.
    if !unsafe { *argv.add(argc) }.is_null() {
        return Args::empty();
    }
    // SAFETY: the environment pointer array follows the argv terminator.
    let envp = unsafe { argv.add(argc + 1) };
    let mut env_count = 0usize;
    while env_count < MAX_ENV {
        // SAFETY: the environment array is NUL-terminated; the scan stops at that terminator
        // or at the MAX_ENV bound.
        if unsafe { *envp.add(env_count) }.is_null() {
            break;
        }
        env_count += 1;
    }
    let auxv = if env_count < MAX_ENV {
        // SAFETY: the auxv array follows the environment terminator.
        unsafe { envp.add(env_count + 1) as *const usize }
    } else {
        core::ptr::null()
    };
    Args {
        argc,
        argv,
        envp,
        env_count,
        auxv,
    }
}

unsafe fn c_string(pointer: *const usize) -> Option<&'static [u8]> {
    if pointer.is_null() {
        return None;
    }
    let bytes = pointer as *const u8;
    let mut length = 0usize;
    while length < MAX_STRING {
        // SAFETY: the C8 trust model says the pointer names a readable string on the process
        // stack; the scan stops at its NUL or at the documented 64 KiB limit.
        if unsafe { *bytes.add(length) } == 0 {
            // SAFETY: the bytes up to `length` were read successfully from the same string.
            return Some(unsafe { core::slice::from_raw_parts(bytes, length) });
        }
        length += 1;
    }
    None
}

struct ArgsIter {
    args: Args,
    index: usize,
}

impl Iterator for ArgsIter {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.args.argc {
            let index = self.index;
            self.index += 1;
            // SAFETY: `Args` was produced by `parse_stack`, which validated `argc` and the
            // argv array; `index < argc` holds.
            let pointer = unsafe { *self.args.argv.add(index) };
            // SAFETY: argv entries name NUL-terminated C8 strings.
            let bytes = (unsafe { c_string(pointer) })?;
            if let Ok(text) = core::str::from_utf8(bytes) {
                return Some(text);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stack {
        words: Vec<usize>,
        strings: Vec<u8>,
    }

    impl Stack {
        fn new() -> Self {
            Self {
                words: Vec::with_capacity(512),
                strings: Vec::with_capacity(4096),
            }
        }

        fn string(&mut self, bytes: &[u8]) -> usize {
            let pointer = self.strings.as_ptr() as usize + self.strings.len();
            self.strings.extend_from_slice(bytes);
            pointer
        }

        fn word(&mut self, value: usize) {
            self.words.push(value);
        }

        fn parse(&self) -> Args {
            // SAFETY: the test builder pushes a well-formed C8 stack into `self.words`.
            unsafe { parse_stack(self.words.as_ptr()) }
        }
    }

    fn simple(argc: usize, args: &[&[u8]]) -> Stack {
        let mut stack = Stack::new();
        let mut pointers = vec![0usize; argc];
        for (index, argument) in args.iter().take(argc).enumerate() {
            pointers[index] = stack.string(argument);
        }
        stack.word(argc);
        for pointer in pointers {
            stack.word(pointer);
        }
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack
    }

    #[test]
    fn argc_zero() {
        let stack = simple(0, &[]);
        let args = stack.parse();
        assert_eq!(args.len(), 0);
        assert_eq!(args.get(0), None);
        assert_eq!(args.iter().count(), 0);
    }

    #[test]
    fn argc_one() {
        let stack = simple(1, &[b"init\0"]);
        let args = stack.parse();
        assert_eq!(args.len(), 1);
        assert_eq!(args.get(0), Some("init"));
    }

    #[test]
    fn argc_three_and_order() {
        let stack = simple(3, &[b"one\0", b"two\0", b"three\0"]);
        let args = stack.parse();
        assert_eq!(args.iter().collect::<Vec<_>>(), vec!["one", "two", "three"]);
    }

    #[test]
    fn empty_strings_are_valid() {
        let stack = simple(2, &[b"\0", b"\0"]);
        let args = stack.parse();
        assert_eq!(args.get(0), Some(""));
        assert_eq!(args.get(1), Some(""));
    }

    #[test]
    fn out_of_range_get_is_none() {
        let stack = simple(1, &[b"one\0"]);
        assert_eq!(stack.parse().get(5), None);
    }

    #[test]
    fn env_with_equals() {
        let mut stack = Stack::new();
        stack.word(1);
        let arg = stack.string(b"prog\0");
        stack.word(arg);
        stack.word(0);
        let env_a = stack.string(b"A=B=C\0");
        let env_b = stack.string(b"EMPTY=\0");
        stack.word(env_a);
        stack.word(env_b);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.env_get("A"), Some("B=C"));
        assert_eq!(args.env_get("EMPTY"), Some(""));
        assert_eq!(args.env_get("MISSING"), None);
    }

    #[test]
    fn auxv_termination_and_lookup() {
        let mut stack = Stack::new();
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(6);
        stack.word(4096);
        stack.word(9);
        stack.word(0x400000);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.auxv(6), Some(4096));
        assert_eq!(args.auxv(9), Some(0x400000));
        assert_eq!(args.auxv(99), None);
    }

    #[test]
    fn non_utf8_arg_get_is_none_but_iteration_continues() {
        let mut stack = Stack::new();
        stack.word(2);
        let bad = stack.string(&[0xff, 0]);
        let good = stack.string(b"good\0");
        stack.word(bad);
        stack.word(good);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.get(0), None);
        assert_eq!(args.iter().collect::<Vec<_>>(), vec!["good"]);
    }

    #[test]
    fn argc_too_large_is_empty() {
        let mut stack = Stack::new();
        stack.word(5000);
        let args = stack.parse();
        assert_eq!(args.len(), 0);
    }

    #[test]
    fn null_argv_pointer_stops_iteration() {
        let mut stack = Stack::new();
        stack.word(2);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.get(0), None);
        assert_eq!(args.iter().count(), 0);
    }

    #[test]
    fn string_without_nul_is_rejected() {
        let mut stack = Stack::new();
        stack.word(1);
        let long = vec![b'x'; MAX_STRING];
        stack.word(long.as_ptr() as usize);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.get(0), None);
    }

    #[test]
    fn auxv_without_terminator_is_bounded() {
        let mut stack = Stack::new();
        stack.word(0);
        stack.word(0);
        stack.word(0);
        for index in 0..100 {
            stack.word(index + 1);
            stack.word(index);
        }
        let args = stack.parse();
        assert_eq!(args.auxv(1), Some(0));
        assert_eq!(args.auxv(1000), None);
    }

    #[test]
    fn misaligned_stack_is_empty() {
        let mut stack = simple(1, &[b"one\0"]);
        let misaligned = (stack.words.as_mut_ptr() as usize + 1) as *const usize;
        // SAFETY: the pointer is deliberately misaligned; `parse_stack` must reject it without
        // dereferencing, which is exactly what this test checks.
        let args = unsafe { parse_stack(misaligned) };
        assert_eq!(args.len(), 0);
    }

    #[test]
    fn missing_argv_terminator_is_rejected() {
        let mut stack = Stack::new();
        stack.word(1);
        let pointer = stack.string(b"one\0");
        stack.word(pointer);
        stack.word(123); // not the required NULL argv terminator
        let args = stack.parse();
        assert_eq!(args.len(), 0);
    }

    #[test]
    fn non_utf8_env_is_skipped() {
        let mut stack = Stack::new();
        stack.word(0);
        stack.word(0);
        let bad = stack.string(b"BAD=\xff\0");
        let good = stack.string(b"GOOD=yes\0");
        stack.word(bad);
        stack.word(good);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.env_get("BAD"), None);
        assert_eq!(args.env_get("GOOD"), Some("yes"));
    }

    #[test]
    fn auxv_zero_key_terminates() {
        let mut stack = Stack::new();
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        stack.word(0);
        let args = stack.parse();
        assert_eq!(args.auxv(1), None);
    }
}
