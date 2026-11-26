// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S744 sysctl conformance probe (test-only, Rust).
//!
//! Runs the declared FreeBSD sysctl MIB subset on the target: the numeric MIBs and the
//! `sysctlbyname`/`sysctlnametomib` forms, the `CTLTYPE_INT`/`CTLTYPE_ULONG`/string widths,
//! `oldp = NULL` sizing, the FreeBSD partial-copy `ENOMEM` rule, malformed names and MIBs, the
//! NULL-pointer results, and the same-time comparison against `uname`, `gethostname` and
//! `sysconf`. Writable `kern.hostname` is exercised through the `sys:` provider, a read-only
//! write must be `EPERM`, and the old value must be copied before the new one is applied.
//!
//! The donor contract is FreeBSD `133344405f` `sys/sys/sysctl.h`,
//! `sys/kern/kern_sysctl.c` (`sysctl_old_user`, `sysctl_handle_int`,
//! `sysctl_handle_string`, `sys___sysctl`) and `sys/kern/kern_mib.c`.
//!
//! The controlled mutations encode wrong beliefs that must turn their row red against the
//! correct product:
//!   required-length -- a too-small integer buffer reports the needed length, not the copied one
//!   unknown-mib     -- an unknown MIB succeeds
//!   privilege       -- writing a read-only MIB succeeds

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{null, null_mut};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "sysctl";

const CTL_KERN: c_int = 1;
const CTL_HW: c_int = 6;
const KERN_OSTYPE: c_int = 1;
const KERN_OSRELEASE: c_int = 2;
const KERN_VERSION: c_int = 4;
const KERN_ARGMAX: c_int = 8;
const KERN_HOSTNAME: c_int = 10;
const KERN_HOSTID: c_int = 11;
const KERN_NISDOMAINNAME: c_int = 22;
const HW_MACHINE: c_int = 1;
const HW_MODEL: c_int = 2;
const HW_NCPU: c_int = 3;
const HW_PHYSMEM: c_int = 5;
const HW_PAGESIZE: c_int = 7;

// FreeBSD `sys/sys/errno.h` values (DR-69 native errno).
const EPERM: c_int = 1;
const ENOENT: c_int = 2;
const ENOMEM: c_int = 12;
const EFAULT: c_int = 14;
const EINVAL: c_int = 22;

// FreeBSD `unistd.h` `_SC_*` selectors used by the independent provider comparison.
const SC_ARG_MAX: c_int = 0;
const SC_PAGESIZE: c_int = 30;

const PROT_NONE: c_int = 0x0;
const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const MAP_PRIVATE: c_int = 0x2;
const MAP_ANON: c_int = 0x20;

/// The declared name -> numeric MIB table.
const MIB_CASES: [(&[u8], &[c_int]); 12] = [
    (b"kern.ostype\0", &[CTL_KERN, KERN_OSTYPE]),
    (b"kern.osrelease\0", &[CTL_KERN, KERN_OSRELEASE]),
    (b"kern.version\0", &[CTL_KERN, KERN_VERSION]),
    (b"kern.hostname\0", &[CTL_KERN, KERN_HOSTNAME]),
    (b"kern.hostid\0", &[CTL_KERN, KERN_HOSTID]),
    (b"kern.domainname\0", &[CTL_KERN, KERN_NISDOMAINNAME]),
    (b"kern.argmax\0", &[CTL_KERN, KERN_ARGMAX]),
    (b"hw.machine\0", &[CTL_HW, HW_MACHINE]),
    (b"hw.model\0", &[CTL_HW, HW_MODEL]),
    (b"hw.ncpu\0", &[CTL_HW, HW_NCPU]),
    (b"hw.physmem\0", &[CTL_HW, HW_PHYSMEM]),
    (b"hw.pagesize\0", &[CTL_HW, HW_PAGESIZE]),
];

#[repr(C)]
struct Utsname {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

impl Utsname {
    const fn zeroed() -> Self {
        Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }
}

unsafe extern "C" {
    fn sysctl(
        mib: *const c_int,
        namelen: c_uint,
        old: *mut c_void,
        oldlenp: *mut usize,
        new: *const c_void,
        newlen: usize,
    ) -> c_int;
    fn sysctlbyname(
        name: *const c_char,
        old: *mut c_void,
        oldlenp: *mut usize,
        new: *const c_void,
        newlen: usize,
    ) -> c_int;
    fn sysctlnametomib(name: *const c_char, mib: *mut c_int, miblenp: *mut usize) -> c_int;
    fn uname(buffer: *mut Utsname) -> c_int;
    fn gethostname(buffer: *mut c_char, length: usize) -> c_int;
    fn sysconf(name: c_int) -> c_long;
    fn mmap(
        address: *mut c_void,
        length: usize,
        protection: c_int,
        flags: c_int,
        fd: c_int,
        offset: i64,
    ) -> *mut c_void;
    fn mprotect(address: *mut c_void, length: usize, protection: c_int) -> c_int;
    fn munmap(address: *mut c_void, length: usize) -> c_int;
    fn __error() -> *mut c_int;
}

fn set_errno(value: c_int) {
    // SAFETY: `__error` returns this thread's writable errno cell.
    unsafe { *__error() = value };
}

fn errno() -> c_int {
    // SAFETY: `__error` returns this thread's readable errno cell.
    unsafe { *__error() }
}

fn field_bytes(field: &[u8; 65]) -> &[u8] {
    let mut end = 0usize;
    while end < field.len() && field[end] != 0 {
        end += 1;
    }
    field.get(..end).unwrap_or(&[])
}

fn name_text(name: &[u8]) -> &str {
    core::str::from_utf8(name).unwrap_or("?").trim_end_matches('\0')
}

fn size_of_name(name: &[u8]) -> Result<usize, c_int> {
    let mut length = 0usize;
    set_errno(0);
    // SAFETY: `name` is NUL terminated and `length` is writable.
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr().cast::<c_char>(),
            null_mut(),
            &mut length,
            null(),
            0,
        )
    };
    if rc == 0 { Ok(length) } else { Err(errno()) }
}

fn read_by_name(name: &[u8], out: &mut [u8]) -> Result<usize, c_int> {
    let mut length = out.len();
    set_errno(0);
    // SAFETY: `name` is NUL terminated and `out` is `out.len()` writable bytes.
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr().cast::<c_char>(),
            out.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    if rc == 0 { Ok(length) } else { Err(errno()) }
}

fn read_by_mib(mib: &[c_int], out: &mut [u8]) -> Result<usize, c_int> {
    let mut length = out.len();
    set_errno(0);
    // SAFETY: `mib` is `mib.len()` readable ints and `out` is `out.len()` writable bytes.
    let rc = unsafe {
        sysctl(
            mib.as_ptr(),
            c_uint::try_from(mib.len()).unwrap_or(0),
            out.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    if rc == 0 { Ok(length) } else { Err(errno()) }
}

fn read_name_value<const N: usize>(name: &[u8]) -> Result<([u8; N], usize), c_int> {
    let size = size_of_name(name)?;
    if size > N {
        return Err(EINVAL);
    }
    let mut buffer = [0u8; N];
    let length = read_by_name(name, buffer.get_mut(..size).unwrap_or(&mut []))?;
    Ok((buffer, length))
}

fn read_mib_value<const N: usize>(mib: &[c_int]) -> Result<([u8; N], usize), c_int> {
    let size = {
        let mut length = 0usize;
        set_errno(0);
        // SAFETY: the MIB is readable and `length` is writable.
        let rc = unsafe {
            sysctl(
                mib.as_ptr(),
                c_uint::try_from(mib.len()).unwrap_or(0),
                null_mut(),
                &mut length,
                null(),
                0,
            )
        };
        if rc == 0 { length } else { return Err(errno()) }
    };
    if size > N {
        return Err(EINVAL);
    }
    let mut buffer = [0u8; N];
    let length = read_by_mib(mib, buffer.get_mut(..size).unwrap_or(&mut []))?;
    Ok((buffer, length))
}

fn decode_i32(value: &[u8], length: usize) -> Option<i32> {
    if length != 4 {
        return None;
    }
    Some(i32::from_ne_bytes(value.get(..4)?.try_into().ok()?))
}

fn text_equals(value: &[u8], length: usize, field: &[u8]) -> bool {
    let Some(slot) = value.get(..length) else {
        return false;
    };
    if slot.len() != field.len().saturating_add(1) {
        return false;
    }
    slot.get(..field.len()) == Some(field) && slot.get(field.len()) == Some(&0)
}

fn write_name(name: &[u8], data: &[u8]) -> c_int {
    set_errno(0);
    // SAFETY: `name` is NUL terminated and `data` is `data.len()` readable bytes.
    unsafe {
        sysctlbyname(
            name.as_ptr().cast::<c_char>(),
            null_mut(),
            null_mut(),
            data.as_ptr().cast::<c_void>(),
            data.len(),
        )
    }
}

fn write_name_with_old(name: &[u8], old: &mut [u8], oldlen: &mut usize, new: &[u8]) -> c_int {
    set_errno(0);
    // SAFETY: `name` and `new` are valid, and `old`/`oldlen` are writable.
    unsafe {
        sysctlbyname(
            name.as_ptr().cast::<c_char>(),
            old.as_mut_ptr().cast::<c_void>(),
            oldlen,
            new.as_ptr().cast::<c_void>(),
            new.len(),
        )
    }
}

fn row_mib_numbers(state: &mut State) {
    let mut okay = true;
    for (name, expected) in MIB_CASES {
        let mut mib = [0i32; 4];
        let mut length = mib.len();
        set_errno(0);
        // SAFETY: `name` is NUL terminated and `mib`/`length` are writable.
        let rc = unsafe {
            sysctlnametomib(
                name.as_ptr().cast::<c_char>(),
                mib.as_mut_ptr(),
                &mut length,
            )
        };
        let got = mib.get(..length).unwrap_or(&[]);
        let matches = rc == 0 && got == expected;
        okay &= matches;
        if !matches {
            conformance::note(state, format_args!("mib-mismatch name={} len={length}", name.len()));
        }
    }
    state.check("mib-numbers", okay, format_args!("declared=12"));
}

fn row_sizing(state: &mut State) {
    let mut okay = true;
    for (name, width) in [
        (b"kern.ostype\0".as_slice(), 0usize),
        (b"kern.osrelease\0".as_slice(), 0),
        (b"kern.version\0".as_slice(), 0),
        (b"kern.hostname\0".as_slice(), 0),
        (b"kern.hostid\0".as_slice(), 8),
        (b"kern.domainname\0".as_slice(), 0),
        (b"kern.argmax\0".as_slice(), 4),
        (b"hw.machine\0".as_slice(), 0),
        (b"hw.model\0".as_slice(), 0),
        (b"hw.ncpu\0".as_slice(), 4),
        (b"hw.physmem\0".as_slice(), 8),
        (b"hw.pagesize\0".as_slice(), 4),
    ] {
        let unsupported = name == b"hw.model\0" || name == b"hw.ncpu\0" || name == b"hw.physmem\0";
        match size_of_name(name) {
            Ok(size) => {
                if unsupported {
                    conformance::note(
                        state,
                        format_args!("sizing {} unexpected size={size}", name_text(name)),
                    );
                    okay = false;
                } else if width == 0 {
                    if size == 0 {
                        conformance::note(state, format_args!("sizing empty text len=0"));
                        okay = false;
                    }
                } else if size != width {
                    conformance::note(
                        state,
                        format_args!("sizing {} size={size} expected={width}", name_text(name)),
                    );
                    okay = false;
                }
            }
            Err(error) => {
                if !unsupported || error != ENOENT {
                    conformance::note(
                        state,
                        format_args!("sizing {} errno={error}", name_text(name)),
                    );
                    okay = false;
                }
            }
        }
    }
    state.check(
        "sizing",
        okay,
        format_args!("widths=int4/ulong8/text-nul unsupported=ENOENT"),
    );
}

fn row_short_buffer(state: &mut State) {
    let mut okay = true;
    let (ncpu, ncpu_len) = read_name_value::<8>(b"kern.argmax\0").unwrap_or(([0; 8], 0));
    let mut short = [0u8; 3];
    let mut length = short.len();
    set_errno(0);
    // SAFETY: the name is NUL terminated and the buffers are writable.
    let rc = unsafe {
        sysctlbyname(
            b"kern.argmax\0".as_ptr().cast::<c_char>(),
            short.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    let int_okay = rc == -1
        && errno() == ENOMEM
        && length == 3
        && &short[..] == &ncpu[..3]
        && ncpu_len == 4;
    if !int_okay {
        conformance::note(
            state,
            format_args!(
                "short-int rc={rc} errno={} len={length} value_len={ncpu_len}",
                errno()
            ),
        );
    }
    okay &= int_okay;

    let (ostype, ostype_len) = read_name_value::<32>(b"kern.ostype\0").unwrap_or(([0; 32], 0));
    let mut short_text = [0u8; 3];
    let mut length = short_text.len();
    set_errno(0);
    // SAFETY: the name is NUL terminated and the buffers are writable.
    let rc = unsafe {
        sysctlbyname(
            b"kern.ostype\0".as_ptr().cast::<c_char>(),
            short_text.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    let text_okay = rc == -1
        && errno() == ENOMEM
        && length == 3
        && &short_text[..] == &ostype[..3]
        && ostype_len > 3;
    if !text_okay {
        conformance::note(
            state,
            format_args!(
                "short-text rc={rc} errno={} len={length} value_len={ostype_len}",
                errno()
            ),
        );
    }
    okay &= text_okay;

    let (physmem, _) = read_name_value::<8>(b"kern.hostid\0").unwrap_or(([0; 8], 0));
    let mut short_ulong = [0u8; 4];
    let mut length = short_ulong.len();
    set_errno(0);
    // SAFETY: the name is NUL terminated and the buffers are writable.
    let rc = unsafe {
        sysctlbyname(
            b"kern.hostid\0".as_ptr().cast::<c_char>(),
            short_ulong.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    let ulong_okay = rc == -1
        && errno() == ENOMEM
        && length == 4
        && &short_ulong[..] == &physmem[..4];
    if !ulong_okay {
        conformance::note(
            state,
            format_args!("short-ulong rc={rc} errno={} len={length}", errno()),
        );
    }
    okay &= ulong_okay;
    state.check(
        "short-buffer",
        okay,
        format_args!("copied_bytes=int3/string3/ulong4 errno=ENOMEM"),
    );
}

fn row_unknown_mib(state: &mut State) {
    let mut length = 0usize;
    set_errno(0);
    // SAFETY: the MIB and length are valid; the value is only a size query.
    let rc = unsafe { sysctl([CTL_KERN, 99].as_ptr(), 2, null_mut(), &mut length, null(), 0) };
    let mut name_length = 0usize;
    set_errno(0);
    // SAFETY: the name is NUL terminated and the length is writable.
    let rc_name = unsafe {
        sysctlbyname(
            b"kern.nosuch\0".as_ptr().cast::<c_char>(),
            null_mut(),
            &mut name_length,
            null(),
            0,
        )
    };
    let name_error = errno();
    let mut mib = [0i32; 4];
    let mut mib_length = mib.len();
    set_errno(0);
    // SAFETY: the name is NUL terminated and the MIB buffers are writable.
    let rc_mib = unsafe {
        sysctlnametomib(
            b"kern.nosuch\0".as_ptr().cast::<c_char>(),
            mib.as_mut_ptr(),
            &mut mib_length,
        )
    };
    let okay = rc == -1
        && errno() == ENOENT
        && rc_name == -1
        && name_error == ENOENT
        && rc_mib == -1
        && errno() == ENOENT;
    state.check("unknown-mib", okay, format_args!("errno=ENOENT"));
}

fn row_malformed_mib(state: &mut State) {
    let mut length = 0usize;
    set_errno(0);
    // SAFETY: the MIB and length are valid; the call is rejected before a value is read.
    let one = unsafe { sysctl([CTL_HW].as_ptr(), 1, null_mut(), &mut length, null(), 0) };
    let one_error = errno();
    let too_long = [CTL_HW; 25];
    set_errno(0);
    // SAFETY: the array and length are valid; the call is rejected before a value is read.
    let long = unsafe { sysctl(too_long.as_ptr(), 25, null_mut(), &mut length, null(), 0) };
    let long_error = errno();
    set_errno(0);
    // SAFETY: the array is valid; the call is rejected before the pointer is used.
    let zero = unsafe { sysctl([CTL_HW, HW_NCPU].as_ptr(), 0, null_mut(), &mut length, null(), 0) };
    let zero_error = errno();
    set_errno(0);
    // SAFETY: the empty name is NUL terminated at its first byte.
    let empty_name = unsafe {
        sysctlbyname(
            b"\0".as_ptr().cast::<c_char>(),
            null_mut(),
            &mut length,
            null(),
            0,
        )
    };
    let empty_error = errno();
    let okay = one == -1
        && one_error == EINVAL
        && long == -1
        && long_error == EINVAL
        && zero == -1
        && zero_error == EINVAL
        && empty_name == -1
        && empty_error == EINVAL;
    state.check("malformed-mib", okay, format_args!("errno=EINVAL"));
}

fn row_invalid_pointers(state: &mut State) {
    let mut length = 8usize;
    set_errno(0);
    // SAFETY: the NULL pointer is the case under test.
    let null_mib = unsafe { sysctl(null(), 2, null_mut(), &mut length, null(), 0) };
    let null_mib_error = errno();
    set_errno(0);
    // SAFETY: the NULL name is the case under test.
    let null_name = unsafe { sysctlbyname(null(), null_mut(), &mut length, null(), 0) };
    let null_name_error = errno();
    set_errno(0);
    // SAFETY: the NULL name is the case under test.
    let null_mib_name = unsafe { sysctlnametomib(null(), null_mut(), &mut length) };
    let null_mib_name_error = errno();

    set_errno(0);
    // SAFETY: `old` and `oldlenp` are both NULL; this is a size query.
    let size_only =
        unsafe { sysctl([CTL_KERN, KERN_ARGMAX].as_ptr(), 2, null_mut(), null_mut(), null(), 0) };

    let mut canary = [0x5Au8; 8];
    set_errno(0);
    // SAFETY: the buffer is writable; `oldlenp` NULL is the case under test.
    let missing_length = unsafe {
        sysctl(
            [CTL_KERN, KERN_ARGMAX].as_ptr(),
            2,
            canary.as_mut_ptr().cast::<c_void>(),
            null_mut(),
            null(),
            0,
        )
    };
    let missing_length_error = errno();

    let mut buffer = [0u8; 8];
    let mut length = 8usize;
    set_errno(0);
    // SAFETY: the buffer and length are writable; `new = NULL` ignores `newlen`.
    let null_new = unsafe {
        sysctl(
            [CTL_KERN, KERN_ARGMAX].as_ptr(),
            2,
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            4,
        )
    };
    let okay = null_mib == -1
        && null_mib_error == EFAULT
        && null_name == -1
        && null_name_error == EFAULT
        && null_mib_name == -1
        && null_mib_name_error == EFAULT
        && size_only == 0
        && missing_length == -1
        && missing_length_error == ENOMEM
        && canary == [0x5A; 8]
        && null_new == 0
        && length == 4;
    if !okay {
        conformance::note(
            state,
            format_args!(
                "invalid null_mib={null_mib}/{} null_name={null_name}/{} null_mib_name={null_mib_name}/{} size_only={size_only} missing={missing_length}/{} canary={} null_new={null_new} len={length}",
                null_mib_error, null_name_error, null_mib_name_error, missing_length_error,
                canary[0]
            ),
        );
    }
    state.check("invalid-pointers", okay, format_args!("null=EFAULT missing-length=ENOMEM"));
}

fn row_dynamic_provider(state: &mut State) {
    let mut uts = Utsname::zeroed();
    // SAFETY: `uts` is a writable `struct utsname`.
    let uname_rc = unsafe { uname(&mut uts) };
    let mut okay = uname_rc == 0;

    for (name, field) in [
        (b"kern.ostype\0".as_slice(), field_bytes(&uts.sysname)),
        (b"kern.osrelease\0".as_slice(), field_bytes(&uts.release)),
        (b"kern.version\0".as_slice(), field_bytes(&uts.version)),
        (b"hw.machine\0".as_slice(), field_bytes(&uts.machine)),
    ] {
        match read_name_value::<128>(name) {
            Ok((value, length)) => {
                let matches = text_equals(&value, length, field);
                if !matches {
                    conformance::note(
                        state,
                        format_args!(
                            "dynamic {} len={length} field={} first={}",
                            name_text(name),
                            field.len(),
                            value.first().copied().unwrap_or(0)
                        ),
                    );
                }
                okay &= matches;
            }
            Err(error) => {
                conformance::note(
                    state,
                    format_args!("dynamic {} errno={error}", name_text(name)),
                );
                okay = false;
            }
        }
    }

    let mut hostname = [0u8; 64];
    // SAFETY: `hostname` is a writable 64-byte buffer.
    let host_rc = unsafe { gethostname(hostname.as_mut_ptr().cast::<c_char>(), hostname.len()) };
    let host_field = {
        let mut end = 0usize;
        while end < hostname.len() && hostname[end] != 0 {
            end += 1;
        }
        hostname.get(..end).unwrap_or(&[])
    };
    match read_name_value::<128>(b"kern.hostname\0") {
        Ok((value, length)) => {
            let matches = host_rc == 0 && text_equals(&value, length, host_field);
            if !matches {
                conformance::note(
                    state,
                    format_args!(
                        "dynamic hostname rc={host_rc} len={length} field={}",
                        host_field.len()
                    ),
                );
            }
            okay &= matches;
        }
        Err(error) => {
            conformance::note(state, format_args!("dynamic hostname errno={error}"));
            okay = false;
        }
    }

    // SAFETY: `sysconf` takes only the selector.
    let pagesize = unsafe { sysconf(SC_PAGESIZE) };
    // SAFETY: `sysconf` takes only the selector.
    let arg_max = unsafe { sysconf(SC_ARG_MAX) };
    let pages = read_name_value::<8>(b"hw.pagesize\0")
        .ok()
        .and_then(|(value, length)| decode_i32(&value, length));
    let argmax = read_name_value::<8>(b"kern.argmax\0")
        .ok()
        .and_then(|(value, length)| decode_i32(&value, length));
    let ncpu_unsupported = size_of_name(b"hw.ncpu\0").err() == Some(ENOENT);
    let physmem_unsupported = size_of_name(b"hw.physmem\0").err() == Some(ENOENT);
    let nums_okay = pages == Some(i32::try_from(pagesize).unwrap_or(-1))
        && argmax == Some(i32::try_from(arg_max).unwrap_or(-1))
        && ncpu_unsupported
        && physmem_unsupported;
    if !nums_okay {
        conformance::note(
            state,
            format_args!(
                "dynamic nums pagesize={pages:?}/{pagesize} argmax={argmax:?}/{arg_max} ncpu_unsupported={ncpu_unsupported} physmem_unsupported={physmem_unsupported}"
            ),
        );
    }
    okay &= nums_okay;

    state.check(
        "dynamic-provider",
        okay,
        format_args!("uname/gethostname/sysconf same-time"),
    );
}

fn row_unsupported_model(state: &mut State) {
    let read_error = size_of_name(b"hw.model\0").err();
    let mut mib = [0i32; 4];
    let mut length = mib.len();
    set_errno(0);
    // SAFETY: the name is NUL terminated and the MIB buffers are writable.
    let rc = unsafe {
        sysctlnametomib(
            b"hw.model\0".as_ptr().cast::<c_char>(),
            mib.as_mut_ptr(),
            &mut length,
        )
    };
    let expected = [CTL_HW, HW_MODEL];
    let okay = read_error == Some(ENOENT)
        && rc == 0
        && length == 2
        && &mib[..2] == &expected[..];
    state.check("unsupported-model", okay, format_args!("read=ENOENT mib=6,2"));
}

fn row_write_readonly(state: &mut State) {
    let mut length = 0usize;
    set_errno(0);
    // SAFETY: the MIB is valid and `new` is one readable byte.
    let ostype = unsafe {
        sysctl(
            [CTL_KERN, KERN_OSTYPE].as_ptr(),
            2,
            null_mut(),
            &mut length,
            b"x".as_ptr().cast::<c_void>(),
            1,
        )
    };
    let ostype_error = errno();
    set_errno(0);
    // SAFETY: the MIB is valid and `new` is one readable byte.
    let hostid = unsafe {
        sysctl(
            [CTL_KERN, KERN_HOSTID].as_ptr(),
            2,
            null_mut(),
            &mut length,
            b"x".as_ptr().cast::<c_void>(),
            1,
        )
    };
    let hostid_error = errno();
    set_errno(0);
    // SAFETY: the MIB is valid and `new` is one readable byte.
    let domain = unsafe {
        sysctl(
            [CTL_KERN, KERN_NISDOMAINNAME].as_ptr(),
            2,
            null_mut(),
            &mut length,
            b"x".as_ptr().cast::<c_void>(),
            1,
        )
    };
    let domain_error = errno();
    let okay = ostype == -1
        && ostype_error == EPERM
        && hostid == -1
        && hostid_error == EPERM
        && domain == -1
        && domain_error == EPERM;
    state.check("write-readonly", okay, format_args!("errno=EPERM"));
}

fn row_hostname_roundtrip(state: &mut State) {
    let original = read_name_value::<128>(b"kern.hostname\0");
    let mut okay = original.is_ok();
    let (original_value, original_length) = original.unwrap_or(([0; 128], 0));

    let first = b"s744-probe";
    let mut old = [0u8; 128];
    let mut old_length = old.len();
    let first_rc = write_name_with_old(b"kern.hostname\0", &mut old, &mut old_length, first);
    let first_errno = errno();
    let old_before_new = first_rc == 0
        && old_length == original_length
        && old[..old_length] == original_value[..original_length];

    let first_read = read_name_value::<128>(b"kern.hostname\0");
    let first_mib = read_mib_value::<128>(&[CTL_KERN, KERN_HOSTNAME]);
    let first_ok = match (&first_read, &first_mib) {
        (Ok((value, length)), Ok((mib_value, mib_length))) => {
            text_equals(value, *length, first) && mib_value[..*mib_length] == value[..*length]
        }
        _ => false,
    };

    let second = b"s744-probe2";
    let mut old = [0u8; 128];
    let mut old_length = old.len();
    let second_rc = write_name_with_old(b"kern.hostname\0", &mut old, &mut old_length, second);
    let second_read = read_name_value::<128>(b"kern.hostname\0");
    let repeated = match &second_read {
        Ok((value, length)) => text_equals(value, *length, second),
        Err(_) => false,
    };

    let restore_length = original_length.saturating_sub(1);
    let restore = write_name(
        b"kern.hostname\0",
        original_value.get(..restore_length).unwrap_or(&[]),
    );
    let restored = match read_name_value::<128>(b"kern.hostname\0") {
        Ok((value, length)) => {
            value[..length] == original_value[..original_length] && restore == 0
        }
        Err(_) => false,
    };

    okay &= old_before_new && first_ok && second_rc == 0 && repeated && restored;
    if !okay {
        conformance::note(
            state,
            format_args!(
                "hostname original_len={original_length} first_rc={first_rc} first_errno={first_errno} old_before_new={old_before_new} first_ok={first_ok} second_rc={second_rc} repeated={repeated} restore={restore} restored={restored}"
            ),
        );
    }
    state.check(
        "hostname-roundtrip",
        okay,
        format_args!("old-before-new repeated-read restored"),
    );
}

fn all_byte(slice: &[u8], value: u8) -> bool {
    slice.iter().all(|byte| *byte == value)
}

fn row_canary(state: &mut State) {
    let before = [0xA5u8; 16];
    let mut value = [0x5Au8; 32];
    let after = [0xA5u8; 16];
    let mut length = 4usize;
    set_errno(0);
    // SAFETY: `value` is a writable 4-byte window; the canaries stay outside it.
    let rc = unsafe {
        sysctlbyname(
            b"kern.argmax\0".as_ptr().cast::<c_char>(),
            value.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    let int_okay = rc == 0
        && length == 4
        && before == [0xA5; 16]
        && after == [0xA5; 16]
        && all_byte(&value[4..], 0x5A);
    if !int_okay {
        conformance::note(
            state,
            format_args!(
                "canary int rc={rc} len={length} tail={} before={} after={}",
                value[4], before[0], after[0]
            ),
        );
    }

    let size = size_of_name(b"kern.ostype\0").unwrap_or(0);
    let mut text_value = [0x5Au8; 32];
    let mut text_length = size;
    set_errno(0);
    // SAFETY: `text_value` is a writable `size`-byte window inside its 32 bytes.
    let text_rc = unsafe {
        sysctlbyname(
            b"kern.ostype\0".as_ptr().cast::<c_char>(),
            text_value.as_mut_ptr().cast::<c_void>(),
            &mut text_length,
            null(),
            0,
        )
    };
    let text_okay = text_rc == 0
        && text_length == size
        && size > 0
        && size <= text_value.len()
        && all_byte(&text_value[size..], 0x5A)
        && before == [0xA5; 16]
        && after == [0xA5; 16];

    state.check("canary", int_okay && text_okay, format_args!("no-overwrite"));
}

fn row_guard_page(state: &mut State) {
    // SAFETY: `sysconf` takes only the selector.
    let page = usize::try_from(unsafe { sysconf(SC_PAGESIZE) }).unwrap_or(4096);
    // SAFETY: a fresh anonymous private mapping of two pages.
    let mapping = unsafe {
        mmap(
            null_mut(),
            page.saturating_mul(2),
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0,
        )
    };
    if mapping.is_null() || mapping as isize == -1 {
        state.check("guard-page", false, format_args!("mmap-failed"));
        return;
    }
    // SAFETY: the second page of our own mapping is made inaccessible.
    let protected = unsafe {
        mprotect(
            mapping.cast::<u8>().add(page).cast::<c_void>(),
            page,
            PROT_NONE,
        )
    };
    let mut okay = protected == 0;
    if okay {
        let size = size_of_name(b"kern.ostype\0").unwrap_or(0);
        if size == 0 || size > page {
            okay = false;
        } else {
            // SAFETY: the buffer ends exactly at the guard page and is `size` bytes long.
            let buffer = unsafe { mapping.cast::<u8>().add(page - size) };
            let mut length = size;
            set_errno(0);
            // SAFETY: `buffer` is `size` writable bytes ending at the guard.
            let rc = unsafe {
                sysctlbyname(
                    b"kern.ostype\0".as_ptr().cast::<c_char>(),
                    buffer.cast::<c_void>(),
                    &mut length,
                    null(),
                    0,
                )
            };
            okay &= rc == 0 && length == size;
            if rc != 0 || length != size {
                conformance::note(
                    state,
                    format_args!("guard text rc={rc} len={length} size={size}"),
                );
            }
        }
        // SAFETY: the buffer ends exactly at the guard page and is four bytes long.
        let int_buffer = unsafe { mapping.cast::<u8>().add(page - 4) };
        let mut length = 4usize;
        set_errno(0);
        // SAFETY: `int_buffer` is four writable bytes ending at the guard.
        let rc = unsafe {
            sysctlbyname(
                b"kern.argmax\0".as_ptr().cast::<c_char>(),
                int_buffer.cast::<c_void>(),
                &mut length,
                null(),
                0,
            )
        };
        okay &= rc == 0 && length == 4;
        if rc != 0 || length != 4 {
            conformance::note(
                state,
                format_args!("guard int rc={rc} len={length}"),
            );
        }
    } else {
        conformance::note(state, format_args!("guard mprotect={protected}"));
    }
    // SAFETY: the mapping is ours and no longer used.
    unsafe { munmap(mapping, page.saturating_mul(2)) };
    state.check("guard-page", okay, format_args!("no-overrun"));
}

fn run_normal() -> i32 {
    let mut state = State::begin(TOPIC);
    row_mib_numbers(&mut state);
    row_sizing(&mut state);
    row_short_buffer(&mut state);
    row_unknown_mib(&mut state);
    row_malformed_mib(&mut state);
    row_invalid_pointers(&mut state);
    row_dynamic_provider(&mut state);
    row_unsupported_model(&mut state);
    row_write_readonly(&mut state);
    row_hostname_roundtrip(&mut state);
    row_canary(&mut state);
    row_guard_page(&mut state);
    state.finish()
}

fn mutation_required_length(state: &mut State) {
    let mut short = [0u8; 3];
    let mut length = short.len();
    set_errno(0);
    // SAFETY: the name is NUL terminated and the buffers are writable.
    let rc = unsafe {
        sysctlbyname(
            b"hw.ncpu\0".as_ptr().cast::<c_char>(),
            short.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    // Wrong belief: a short integer buffer reports the needed width (4), not the copied bytes.
    let wrong = rc == -1 && errno() == ENOMEM && length == 4;
    state.check("short-buffer", wrong, format_args!("oldlen={length}"));
}

fn mutation_unknown_mib(state: &mut State) {
    let mut buffer = [0u8; 8];
    let mut length = buffer.len();
    set_errno(0);
    // SAFETY: the MIB and buffers are valid.
    let rc = unsafe {
        sysctl(
            [CTL_KERN, 99].as_ptr(),
            2,
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut length,
            null(),
            0,
        )
    };
    // Wrong belief: an unknown MIB succeeds.
    state.check("unknown-mib", rc == 0, format_args!("rc={rc}"));
}

fn mutation_privilege(state: &mut State) {
    let rc = write_name(b"kern.ostype\0", b"x");
    // Wrong belief: writing a read-only MIB succeeds.
    state.check("write-readonly", rc == 0, format_args!("rc={rc}"));
}

fn run_mutation(name: &str) -> bool {
    let mut state = State {
        topic: TOPIC,
        total: 0,
        passed: 0,
        failed: 0,
    };
    conformance::mutation_begin(TOPIC, name);
    match name {
        "required-length" => mutation_required_length(&mut state),
        "unknown-mib" => mutation_unknown_mib(&mut state),
        "privilege" => mutation_privilege(&mut state),
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name) != 0
}

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    let mut failures = run_normal();
    for name in ["required-length", "unknown-mib", "privilege"] {
        if !run_mutation(name) {
            failures += 1;
        }
    }
    if failures == 0 { 0 } else { 1 }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    conformance::emit(format_args!("CONFORM {TOPIC} panic\n"));
    // SAFETY: `_exit` never returns and terminates the probe.
    unsafe { _exit(101) }
}

unsafe extern "C" {
    fn _exit(status: c_int) -> !;
}
