// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Adversarial-argument rows for the ravenlibc login/server wrappers.
//!
//! Each test gives the platform/pure function one garbage vector, asserts the documented
//! `Result`/`errno`, then proves the process is still usable with a legal follow-up call.

use raven_libc_abi::errno::Errno as AbiErrno;
use raven_libc_abi::fcntl;
use raven_libc_abi::mman;
use raven_libc_abi::socket::MSG_OOB;
use raven_libc_abi::structs::{Pollfd, Timespec, Timeval};
use raven_libc_net::ErrnoExt as _;
use raven_libc_net::flags::{recv_supported, send_supported};
use raven_libc_platform::poll::{self, MAX_POLL_FDS};
use raven_libc_platform::{Errno, FakeSys, Platform, mem};

/// Build the C-library `Errno` newtype from a generated number.
const fn abi(code: i32) -> AbiErrno {
    AbiErrno(code as u16)
}

fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.file(b"/etc/motd", b"hello");
    fake.dir(b"/tmp");
    Platform::new(fake)
}

fn open_motd(platform: &Platform<FakeSys>, flags: i32) -> i32 {
    platform
        .open(b"/etc/motd", flags as u32, 0)
        .expect("open /etc/motd")
}

#[test]
fn read_rejects_bad_fds_and_stays_usable() {
    let platform = platform();
    let mut byte = [0u8; 1];
    assert_eq!(platform.read(-1, &mut byte), Err(Errno::EBADF));
    assert_eq!(platform.read(i32::MAX, &mut byte), Err(Errno::EBADF));
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    let mut buffer = [0u8; 5];
    assert_eq!(platform.read(fd, &mut buffer), Ok(5));
    assert_eq!(&buffer, b"hello");
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn write_rejects_bad_fds_and_stays_usable() {
    let platform = platform();
    assert_eq!(platform.write(-1, b"x"), Err(Errno::EBADF));
    assert_eq!(platform.write(i32::MAX, b"x"), Err(Errno::EBADF));
    let fd = open_motd(&platform, fcntl::O_RDWR);
    assert_eq!(platform.write(fd, b"!"), Ok(1));
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn open_rejects_empty_long_nul_and_garbage_flags() {
    let platform = platform();
    assert_eq!(
        platform.open(b"", fcntl::O_RDONLY as u32, 0),
        Err(Errno::ENOENT)
    );
    let long = vec![b'a'; 4097];
    assert_eq!(
        platform.open(&long, fcntl::O_RDONLY as u32, 0),
        Err(Errno::ENAMETOOLONG)
    );
    assert_eq!(
        platform.open(b"/etc/motd\0", fcntl::O_RDONLY as u32, 0),
        Err(Errno::EINVAL)
    );
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn openat_rejects_bad_dirfds() {
    let platform = platform();
    assert_eq!(
        platform.openat(-1, b"motd", fcntl::O_RDONLY as u32, 0),
        Err(Errno::EBADF)
    );
    assert_eq!(
        platform.openat(i32::MAX, b"motd", fcntl::O_RDONLY as u32, 0),
        Err(Errno::EBADF)
    );
    let fd = platform
        .openat(fcntl::AT_FDCWD, b"/etc/motd", fcntl::O_RDONLY as u32, 0)
        .expect("openat /etc/motd");
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn fstat_rejects_bad_fds() {
    let platform = platform();
    assert_eq!(platform.fstat(-1), Err(Errno::EBADF));
    assert_eq!(platform.fstat(i32::MAX), Err(Errno::EBADF));
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert!(platform.fstat(fd).is_ok());
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn close_rejects_bad_and_double_close() {
    let platform = platform();
    assert_eq!(platform.close(-1), Err(Errno::EBADF));
    assert_eq!(platform.close(i32::MAX), Err(Errno::EBADF));
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(platform.close(fd), Ok(()));
    assert_eq!(platform.close(fd), Err(Errno::EBADF));
}

#[test]
fn dup_rejects_bad_fds_and_out_of_range_min_fd() {
    let platform = platform();
    assert_eq!(platform.dup(-1, 0, false), Err(Errno::EBADF));
    assert_eq!(platform.dup(i32::MAX, 0, false), Err(Errno::EBADF));
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(platform.dup(fd, u32::MAX, false), Err(Errno::EINVAL));
    let copy = platform.dup(fd, 0, false).expect("dup");
    assert_eq!(platform.close(copy), Ok(()));
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn dup2_rejects_bad_old_and_new() {
    let platform = platform();
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(platform.dup2(-1, 3, false), Err(Errno::EBADF));
    assert_eq!(platform.dup2(fd, -1, false), Err(Errno::EBADF));
    assert_eq!(platform.dup2(fd, i32::MAX, false), Err(Errno::EBADF));
    assert_eq!(platform.dup2(fd, fd, false), Ok(fd));
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn fcntl_rejects_bad_fd_command_and_dupfd_argument() {
    let platform = platform();
    assert_eq!(platform.fcntl(-1, fcntl::F_GETFD, 0), Err(Errno::EBADF));
    assert_eq!(
        platform.fcntl(i32::MAX, fcntl::F_GETFD, 0),
        Err(Errno::EBADF)
    );
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(platform.fcntl(fd, i32::MIN, 0), Err(Errno::EINVAL));
    assert_eq!(platform.fcntl(fd, fcntl::F_DUPFD, -1), Err(Errno::EINVAL));
    assert_eq!(platform.fcntl(fd, fcntl::F_GETFD, 0), Ok(0));
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn poll_rejects_oversized_array_and_marks_bad_fds() {
    let platform = platform();
    let mut too_many = vec![Pollfd::ZERO; MAX_POLL_FDS + 1];
    assert_eq!(poll::poll(&platform, &mut too_many, 0), Err(Errno::EINVAL));
    let mut negative = [Pollfd {
        fd: -1,
        events: 0,
        revents: -1,
    }];
    assert_eq!(poll::poll(&platform, &mut negative, 0), Ok(0));
    assert_eq!(negative[0].revents, 0);
    let mut invalid = [Pollfd {
        fd: i32::MAX,
        events: 0,
        revents: 0,
    }];
    assert_eq!(poll::poll(&platform, &mut invalid, 0), Ok(1));
    assert_eq!(invalid[0].revents, raven_libc_abi::poll::POLLNVAL as i16);
}

#[test]
fn ppoll_rejects_negative_and_out_of_range_timeouts() {
    let platform = platform();
    let mut fds: [Pollfd; 0] = [];
    let negative_sec = Timespec {
        tv_sec: -1,
        tv_nsec: 0,
    };
    assert_eq!(
        poll::ppoll(&platform, &mut fds, Some(&negative_sec), None),
        Err(Errno::EINVAL)
    );
    let negative_nsec = Timespec {
        tv_sec: 0,
        tv_nsec: -1,
    };
    assert_eq!(
        poll::ppoll(&platform, &mut fds, Some(&negative_nsec), None),
        Err(Errno::EINVAL)
    );
    let too_big_nsec = Timespec {
        tv_sec: 0,
        tv_nsec: 1_000_000_000,
    };
    assert_eq!(
        poll::ppoll(&platform, &mut fds, Some(&too_big_nsec), None),
        Err(Errno::EINVAL)
    );
    let zero = Timespec::ZERO;
    assert_eq!(poll::ppoll(&platform, &mut fds, Some(&zero), None), Ok(0));
}

#[test]
fn select_rejects_negative_and_huge_nfds_and_bad_timeouts() {
    let platform = platform();
    let mut read = [0u64; 16];
    let mut write = [0u64; 16];
    let mut except = [0u64; 16];
    assert_eq!(
        poll::select(&platform, -1, &mut read, &mut write, &mut except, None),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        poll::select(
            &platform,
            i32::MAX,
            &mut read,
            &mut write,
            &mut except,
            None
        ),
        Err(Errno::EINVAL)
    );
    let bad = Timeval {
        tv_sec: 0,
        tv_usec: 1_000_000,
    };
    assert_eq!(
        poll::select(&platform, 0, &mut read, &mut write, &mut except, Some(&bad)),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        poll::select(&platform, 0, &mut read, &mut write, &mut except, None),
        Ok(0)
    );
}

#[test]
fn mmap_rejects_zero_huge_bad_offset_prot_and_type() {
    let platform = platform();
    assert_eq!(
        mem::mmap(&platform, 0, 0, mman::PROT_READ, mman::MAP_PRIVATE, -1, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mem::mmap(
            &platform,
            0,
            usize::MAX,
            mman::PROT_READ,
            mman::MAP_PRIVATE,
            -1,
            0
        ),
        Err(Errno::ENOMEM)
    );
    assert_eq!(
        mem::mmap(
            &platform,
            0,
            4096,
            mman::PROT_READ,
            mman::MAP_PRIVATE,
            -1,
            -1
        ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mem::mmap(
            &platform,
            0,
            4096,
            mman::PROT_READ,
            mman::MAP_PRIVATE,
            -1,
            1
        ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mem::mmap(&platform, 0, 4096, 0x80, mman::MAP_PRIVATE, -1, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mem::mmap(
            &platform,
            0,
            4096,
            mman::PROT_WRITE | mman::PROT_EXEC,
            mman::MAP_PRIVATE,
            -1,
            0
        ),
        Err(Errno::EACCES)
    );
    assert_eq!(
        mem::mmap(
            &platform,
            0,
            4096,
            mman::PROT_READ,
            mman::MAP_SHARED | mman::MAP_PRIVATE,
            -1,
            0
        ),
        Err(Errno::EINVAL)
    );
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(
        mem::mmap(&platform, 0, 4096, mman::PROT_READ, 0, fd, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn munmap_rejects_zero_and_unaligned_and_stays_usable() {
    let platform = platform();
    assert_eq!(mem::munmap(&platform, 0, 0), Err(Errno::EINVAL));
    assert_eq!(mem::munmap(&platform, 1, 4096), Err(Errno::EINVAL));
    let address = mem::mmap(
        &platform,
        0,
        4096,
        mman::PROT_READ,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("anonymous mmap");
    assert_eq!(mem::munmap(&platform, address, 4096), Ok(()));
}

#[test]
fn mprotect_rejects_zero_unaligned_invalid_and_wx() {
    let platform = platform();
    assert_eq!(
        mem::mprotect(&platform, 0, 0, mman::PROT_READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mem::mprotect(&platform, 1, 4096, mman::PROT_READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(mem::mprotect(&platform, 0, 4096, 0x80), Err(Errno::EINVAL));
    assert_eq!(
        mem::mprotect(&platform, 0, 4096, mman::PROT_WRITE | mman::PROT_EXEC),
        Err(Errno::EACCES)
    );
    let address = mem::mmap(
        &platform,
        0,
        4096,
        mman::PROT_READ,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("anonymous mmap");
    assert_eq!(
        mem::mprotect(&platform, address, 4096, mman::PROT_READ),
        Ok(())
    );
    assert_eq!(mem::munmap(&platform, address, 4096), Ok(()));
}

#[test]
fn lseek_rejects_bad_fd_and_whence() {
    let platform = platform();
    assert_eq!(platform.lseek(-1, 0, 0), Err(Errno::EBADF));
    assert_eq!(platform.lseek(i32::MAX, 0, 0), Err(Errno::EBADF));
    let fd = open_motd(&platform, fcntl::O_RDONLY);
    assert_eq!(platform.lseek(fd, 0, i32::MAX), Err(Errno::EINVAL));
    assert_eq!(platform.lseek(fd, 0, 0), Ok(0));
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn pipe2_rejects_unknown_flags_and_platform_stays_usable() {
    let platform = platform();
    assert_eq!(platform.pipe2(i32::MIN), Err(Errno::EINVAL));
    assert_eq!(platform.pipe2(-1), Err(Errno::EINVAL));
    let mut buffer = [0u8; 4096];
    let used = platform.getcwd_into(&mut buffer).expect("getcwd");
    assert_eq!(buffer[used - 1], 0);
}

#[test]
fn getcwd_rejects_a_short_buffer_and_stays_usable() {
    let platform = platform();
    assert_eq!(platform.getcwd_into(&mut []), Err(Errno::ERANGE));
    let mut buffer = [0u8; 4096];
    let used = platform.getcwd_into(&mut buffer).expect("getcwd");
    assert_eq!(buffer[used - 1], 0);
}

#[test]
fn stat_rejects_empty_long_and_nul_paths() {
    let platform = platform();
    assert_eq!(platform.stat(b""), Err(Errno::ENOENT));
    let long = vec![b'a'; 4097];
    assert_eq!(platform.stat(&long), Err(Errno::ENAMETOOLONG));
    assert_eq!(platform.stat(b"/etc/motd\0"), Err(Errno::EINVAL));
    assert!(platform.stat(b"/etc/motd").is_ok());
}

#[test]
fn chdir_rejects_empty_long_and_nul_paths() {
    let platform = platform();
    assert_eq!(platform.chdir(b""), Err(Errno::ENOENT));
    let long = vec![b'a'; 4097];
    assert_eq!(platform.chdir(&long), Err(Errno::ENAMETOOLONG));
    assert_eq!(platform.chdir(b"/tmp\0"), Err(Errno::EINVAL));
    assert_eq!(platform.chdir(b"/tmp"), Ok(()));
}

#[test]
fn strtol_rejects_invalid_bases_and_empty_input() {
    use raven_libc_stdlib::conv::{IntStatus, strtol};
    let (_, used, status) = strtol(b"", 10);
    assert_eq!(used, 0);
    assert!(status == IntStatus::NoDigits);
    let (_, used, status) = strtol(b"-", 10);
    assert_eq!(used, 0);
    assert!(status == IntStatus::NoDigits);
    for base in [1, -1, 37, i32::MAX, i32::MIN] {
        let (_, _, status) = strtol(b"1", base);
        assert!(status == IntStatus::InvalidBase, "base {base}");
    }
    let (value, used, status) = strtol(b"42", 10);
    assert_eq!((value, used), (42, 2));
    assert!(status == IntStatus::Ok);
}

#[test]
fn strtoul_rejects_invalid_bases_and_empty_input() {
    use raven_libc_stdlib::conv::{IntStatus, strtoul};
    let (_, used, status) = strtoul(b"", 10);
    assert_eq!(used, 0);
    assert!(status == IntStatus::NoDigits);
    let (_, _, status) = strtoul(b"1", 1);
    assert!(status == IntStatus::InvalidBase);
    let (_, _, status) = strtoul(b"1", -1);
    assert!(status == IntStatus::InvalidBase);
    let (_, _, status) = strtoul(b"1", 37);
    assert!(status == IntStatus::InvalidBase);
    let (value, used, status) = strtoul(b"7", 10);
    assert_eq!((value, used), (7, 1));
    assert!(status == IntStatus::Ok);
}

#[test]
fn qsort_survives_zero_elements_and_zero_element_size() {
    let mut empty: [u8; 0] = [];
    raven_libc_stdlib::qsort(&mut empty, 0, |_, _| 0);
    let mut bytes = [3u8, 1, 2];
    raven_libc_stdlib::qsort(&mut bytes, 0, |_, _| 0);
    assert_eq!(bytes, [3, 1, 2]);
    raven_libc_stdlib::qsort(&mut bytes, 1, |a, b| match a[0].cmp(&b[0]) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    });
    assert_eq!(bytes, [1, 2, 3]);
}

#[test]
fn getenv_setenv_reject_empty_and_equal_names() {
    use raven_libc_stdlib::env::{EnvTable, getenv, setenv};
    let mut env = EnvTable::new();
    assert_eq!(getenv(&env, b""), None);
    assert_eq!(getenv(&env, b"=x"), None);
    assert_eq!(
        setenv(&mut env, b"", b"x", true),
        Err(abi(raven_libc_abi::errno::EINVAL))
    );
    assert_eq!(
        setenv(&mut env, b"=x", b"y", true),
        Err(abi(raven_libc_abi::errno::EINVAL))
    );
    setenv(&mut env, b"PATH", b"/bin", true).expect("setenv");
    assert_eq!(getenv(&env, b"PATH"), Some(&b"/bin"[..]));
}

#[test]
fn realpath_rejects_short_buffer_empty_and_long_path() {
    use raven_libc_stdlib::path::{FileKind, Walker, realpath};
    struct Walk;
    impl Walker for Walk {
        fn lstat_kind(&mut self, path: &[u8]) -> Result<FileKind, AbiErrno> {
            match path {
                b"/" | b"/tmp" | b"/etc" => Ok(FileKind::Dir),
                b"/etc/motd" => Ok(FileKind::Regular),
                _ => Err(abi(raven_libc_abi::errno::ENOENT)),
            }
        }
        fn readlink(&mut self, _path: &[u8], _buf: &mut [u8]) -> Result<usize, AbiErrno> {
            Err(abi(raven_libc_abi::errno::EINVAL))
        }
        fn getcwd(&mut self, buf: &mut [u8]) -> Result<usize, AbiErrno> {
            buf[0] = b'/';
            Ok(1)
        }
    }
    let mut out = vec![0u8; raven_libc_abi::limits::PATH_MAX as usize];
    assert_eq!(
        realpath(&mut Walk, b"/tmp", &mut out[..8]),
        Err(abi(raven_libc_abi::errno::EINVAL))
    );
    assert_eq!(
        realpath(&mut Walk, b"", &mut out),
        Err(abi(raven_libc_abi::errno::ENOENT))
    );
    let long = vec![b'a'; 5000];
    assert_eq!(
        realpath(&mut Walk, &long, &mut out),
        Err(abi(raven_libc_abi::errno::ENAMETOOLONG))
    );
    let used = realpath(&mut Walk, b"/tmp", &mut out).expect("realpath /tmp");
    assert_eq!(&out[..used], b"/tmp");
}

struct Va {
    values: Vec<u64>,
    index: usize,
}

impl raven_libc_fmt::traits::VaReader for Va {
    fn next_u64(&mut self) -> u64 {
        let value = self.values.get(self.index).copied().unwrap_or(0);
        self.index = self.index.saturating_add(1);
        value
    }
    fn next_f64_bits(&mut self) -> u64 {
        0
    }
    fn next_ld_bits(&mut self) -> u128 {
        0
    }
    fn clone_state(&self) -> Self {
        Self {
            values: self.values.clone(),
            index: self.index,
        }
    }
}

impl raven_libc_fmt::traits::FmtEnv for Va {
    fn cstr_len(&mut self, _ptr: usize, _max: usize) -> usize {
        0
    }
    fn read_bytes(&mut self, _ptr: usize, _offset: usize, _buf: &mut [u8]) -> usize {
        0
    }
    fn wstr_len(&mut self, _ptr: usize, _max: usize) -> usize {
        0
    }
    fn read_wchars(&mut self, _ptr: usize, _offset: usize, _buf: &mut [u32]) -> usize {
        0
    }
    fn store_count(&mut self, _ptr: usize, _size: raven_libc_fmt::types::StoreSize, _value: i64) {}
    fn errno_text(&mut self, _errnum: i32, _buf: &mut [u8; 64]) -> usize {
        0
    }
    fn store_int(&mut self, _ptr: usize, _size: raven_libc_fmt::types::StoreSize, _value: u64) {}
    fn store_chars(&mut self, _ptr: usize, _data: &[u8]) {}
    fn store_wchars(&mut self, _ptr: usize, _data: &[u32]) {}
    fn store_float_bits(
        &mut self,
        _ptr: usize,
        _kind: raven_libc_fmt::types::FloatKind,
        _bits: u128,
    ) {
    }
    fn alloc_store(&mut self, _dest_ptr: usize, _bytes: &[u8], _wide: bool) -> bool {
        false
    }
}

#[test]
fn snprintf_survives_null_empty_and_unterminated_formats() {
    use raven_libc_fmt::glue::snprintf::vsnprintf_core;
    use raven_libc_fmt::types::{FmtErr, FmtOptions};
    let opts = FmtOptions::default();
    let mut va = Va {
        values: vec![42],
        index: 0,
    };
    let mut out = [0xAAu8; 16];
    let result = vsnprintf_core(&mut out, 16, b"", &mut va, &opts, &raven_libc_fmt::NoFloat);
    assert_eq!(result.ok(), Some(0));
    assert_eq!(out[0], 0);
    let mut va = Va {
        values: vec![42],
        index: 0,
    };
    let mut out = [0xAAu8; 16];
    let result = vsnprintf_core(&mut out, 16, b"%", &mut va, &opts, &raven_libc_fmt::NoFloat);
    // A trailing `%` is undefined by POSIX. The engine stops at the implicit NUL and writes
    // nothing; the contract here is only that it must not panic or abort.
    assert_eq!(result.ok(), Some(0));
    assert_eq!(out[0], 0);
    let mut va = Va {
        values: vec![0],
        index: 0,
    };
    let mut out = [0xAAu8; 16];
    let result = vsnprintf_core(
        &mut out,
        16,
        b"%s",
        &mut va,
        &opts,
        &raven_libc_fmt::NoFloat,
    );
    // A null `%s` is undefined in C; this engine follows the common "(null)" rendering and
    // must not dereference the pointer.
    assert_eq!(result.ok(), Some(6));
    assert_eq!(&out[..6], b"(null)");
    let mut va = Va {
        values: vec![42],
        index: 0,
    };
    let mut out = [0xAAu8; 8];
    let result = vsnprintf_core(
        &mut out,
        usize::MAX,
        b"%d",
        &mut va,
        &opts,
        &raven_libc_fmt::NoFloat,
    );
    assert!(matches!(result, Err(FmtErr::Overflow)));
    assert_eq!(out[0], 0);
    let mut va = Va {
        values: vec![42],
        index: 0,
    };
    let mut out = [0u8; 16];
    let result = vsnprintf_core(
        &mut out,
        16,
        b"%d",
        &mut va,
        &opts,
        &raven_libc_fmt::NoFloat,
    );
    assert_eq!(result.ok(), Some(2));
    assert_eq!(&out[..3], b"42\0");
}

#[test]
fn sigaction_rejects_invalid_signals_and_uncatchable_ones() {
    use raven_libc_signal::dispositions::{DispTable, Disposition};
    use raven_libc_signal::sigaction;
    let table = DispTable::new();
    for sig in [0, -1, 32, 33, 34, 65, i32::MAX] {
        assert_eq!(
            sigaction(&table, sig, None, None),
            Err(abi(raven_libc_abi::errno::EINVAL))
        );
    }
    for sig in [
        raven_libc_abi::generated::signal::SIGKILL,
        raven_libc_abi::generated::signal::SIGSTOP,
    ] {
        assert_eq!(
            sigaction(&table, sig, Some(&Disposition::DEFAULT), None),
            Err(abi(raven_libc_abi::errno::EINVAL))
        );
    }
    assert_eq!(sigaction(&table, 7, None, None), Ok(()));
    assert_eq!(
        sigaction(&table, 7, Some(&Disposition::DEFAULT), None),
        Ok(())
    );
}

struct SignalKernel;

impl raven_libc_signal::kernel::SigKernel for SignalKernel {
    fn caps(&self) -> raven_libc_signal::kernel::SigCaps {
        raven_libc_signal::kernel::SigCaps::PRODUCTION
    }
    fn thread_pointer(&self) -> usize {
        0
    }
    fn thread_handle(&self) -> u32 {
        0
    }
    fn yield_now(&self) {}
    fn now_ns(&self) -> u64 {
        0
    }
    fn kick(&self, _handle: u32, _bits: u32) -> Result<(), AbiErrno> {
        Ok(())
    }
    fn resend(
        &self,
        _sig: i32,
        _info: &raven_libc_abi::structs::SiginfoT,
        _mask: u64,
    ) -> Result<(), AbiErrno> {
        Err(abi(raven_libc_abi::errno::ENOSYS))
    }
    fn proc_kill(
        &self,
        _pid: i32,
        _sig: i32,
        _info: Option<&raven_libc_abi::structs::SiginfoT>,
    ) -> Result<(), AbiErrno> {
        Err(abi(raven_libc_abi::errno::ENOSYS))
    }
    fn take_process_pending(
        &self,
        _deliverable: u64,
    ) -> Option<(i32, raven_libc_abi::structs::SiginfoT)> {
        None
    }
    fn wait_for_kick(&self, _deadline_ns: Option<u64>) -> Result<(), AbiErrno> {
        Ok(())
    }
    fn terminate(&self, _sig: i32, _core: bool) -> ! {
        std::process::abort()
    }
}

#[test]
fn sigprocmask_rejects_an_unknown_how() {
    use raven_libc_signal::maskops::sigprocmask;
    let kernel = SignalKernel;
    for how in [3, -1, i32::MAX, i32::MIN] {
        assert_eq!(
            sigprocmask(&kernel, how, None, None),
            Err(abi(raven_libc_abi::errno::EINVAL))
        );
    }
}

struct Wait {
    seen: Option<(i32, u32)>,
}

impl raven_libc_proc::wait::Wait4 for Wait {
    fn wait4(
        &mut self,
        pid: i32,
        options: u32,
        _rusage: Option<&mut [u8; 144]>,
    ) -> Result<i32, raven_libc_proc::Errno> {
        self.seen = Some((pid, options));
        if pid == i32::MAX {
            Err(raven_libc_abi::errno::ECHILD)
        } else {
            Ok(pid)
        }
    }
}

#[test]
fn waitpid_passes_garbage_pids_without_panicking() {
    use raven_libc_proc::waitpid::waitpid;
    let mut wait = Wait { seen: None };
    assert_eq!(
        waitpid(&mut wait, i32::MAX, 0),
        Err(raven_libc_abi::errno::ECHILD)
    );
    assert_eq!(wait.seen, Some((i32::MAX, 0)));
    assert_eq!(waitpid(&mut wait, i32::MIN, u32::MAX), Ok(i32::MIN));
    assert_eq!(wait.seen, Some((i32::MIN, u32::MAX)));
    assert_eq!(waitpid(&mut wait, -1, 0), Ok(-1));
    assert_eq!(waitpid(&mut wait, 42, 0), Ok(42));
}

#[test]
fn recv_send_flag_validation_rejects_unknown_and_oob() {
    assert_eq!(recv_supported(i32::MIN), Err(AbiErrno::EINVAL));
    assert_eq!(recv_supported(0x0800_0000), Err(AbiErrno::EINVAL));
    assert_eq!(recv_supported(MSG_OOB), Err(AbiErrno::EOPNOTSUPP));
    assert_eq!(send_supported(i32::MIN), Err(AbiErrno::EINVAL));
    assert_eq!(send_supported(0x0800_0000), Err(AbiErrno::EINVAL));
    assert_eq!(send_supported(MSG_OOB), Err(AbiErrno::EOPNOTSUPP));
    assert_eq!(recv_supported(0), Ok(()));
    assert_eq!(send_supported(0), Ok(()));
}
