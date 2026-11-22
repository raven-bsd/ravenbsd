// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Direct nextest coverage for 20 SSH-critical donor rows.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::missing_safety_doc,
    unexpected_cfgs,
    unsafe_code
)]

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::alloc::Layout;
use std::sync::Mutex;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::JmpBuf;
use raven_libc_compat::pty::openpty;
use raven_libc_compat::seams::{ForkResult, PtyOps};
use raven_libc_stdio::state::State;
use raven_libc_stdio::traits::{Lock, StatLite, SysFile};
use raven_libc_stdio::{fclose, fcloseall, fflush, file_from_ptr, fopen, fread, fwrite};

// --- string / memory paths (SSH packet buffers, C5-C7) -----------------------------

#[test]
fn str_bsd_compat_bcopy_bzero_index() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/bsd_compat.rs
    let src = *b"raven";
    let mut dst = [0u8; 5];
    unsafe {
        raven_libc_str::raw::bcopy(src.as_ptr(), dst.as_mut_ptr(), 5);
    }
    assert_eq!(dst, src);
    unsafe {
        raven_libc_str::raw::bzero(dst.as_mut_ptr(), 5);
    }
    assert_eq!(dst, [0u8; 5]);
    let hay = b"needle\0";
    unsafe {
        assert_eq!(
            raven_libc_str::raw::index(hay.as_ptr(), i32::from(b'n')),
            hay.as_ptr() as *mut u8
        );
        assert_eq!(
            raven_libc_str::raw::rindex(hay.as_ptr(), i32::from(b'e')),
            hay.as_ptr().add(5) as *mut u8
        );
        assert!(raven_libc_str::raw::index(hay.as_ptr(), i32::from(b'z')).is_null());
    }
}

#[test]
fn str_explicit_bzero_clears() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/explicit_bzero.rs
    let mut buffer = [0x5au8; 8];
    unsafe {
        raven_libc_str::raw::explicit_bzero(buffer.as_mut_ptr(), buffer.len());
    }
    assert_eq!(buffer, [0u8; 8]);
}

#[test]
fn str_memccpy_stops_at_byte() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/memccpy.rs
    let src = b"abc\0def";
    let mut dst = [0xffu8; 7];
    let end = unsafe { raven_libc_str::raw::memccpy(dst.as_mut_ptr(), src.as_ptr(), 0, 6) };
    assert_eq!(&dst[..4], b"abc\0");
    assert_eq!(end, unsafe { dst.as_mut_ptr().add(4) });
    let mut miss = [0xffu8; 3];
    let none = unsafe { raven_libc_str::raw::memccpy(miss.as_mut_ptr(), b"abc".as_ptr(), 0, 3) };
    assert!(none.is_null());
}

#[test]
fn str_memmem_finds_needle() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/memmem.rs
    let hay = b"xxneedlezz";
    let needle = b"needle";
    let found = unsafe {
        raven_libc_str::raw::memmem(hay.as_ptr(), hay.len(), needle.as_ptr(), needle.len())
    };
    assert_eq!(found, unsafe { hay.as_ptr().add(2) as *mut u8 });
    let missing =
        unsafe { raven_libc_str::raw::memmem(hay.as_ptr(), hay.len(), b"absent".as_ptr(), 6) };
    assert!(missing.is_null());
    let empty = unsafe { raven_libc_str::raw::memmem(hay.as_ptr(), hay.len(), hay.as_ptr(), 0) };
    assert_eq!(empty, hay.as_ptr() as *mut u8);
}

#[test]
fn str_mem_mod_memmove_overlap() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/mem/mod.rs
    let mut buffer = *b"abcdef";
    let pointer = buffer.as_mut_ptr();
    unsafe {
        raven_libc_str::raw::memmove(pointer.add(1), pointer, 5);
    }
    assert_eq!(&buffer, b"aabcde");
    let mut filled = [0u8; 4];
    unsafe {
        raven_libc_str::raw::memset(filled.as_mut_ptr(), 0x7f, filled.len());
    }
    assert_eq!(filled, [0x7f; 4]);
    assert_eq!(
        unsafe { raven_libc_str::raw::memcmp(b"abc".as_ptr(), b"abc".as_ptr(), 3) },
        0
    );
}

#[test]
fn str_mempcpy_returns_end() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/mempcpy.rs
    let src = *b"ssh";
    let mut dst = [0u8; 3];
    let end = unsafe { raven_libc_str::raw::mempcpy(dst.as_mut_ptr(), src.as_ptr(), src.len()) };
    assert_eq!(dst, src);
    assert_eq!(end, unsafe { dst.as_mut_ptr().add(3) });
}

#[test]
fn str_memset_s_bounds_and_fill() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/memset_s.rs
    let mut buffer = [0u8; 4];
    assert_eq!(
        unsafe { raven_libc_str::raw::memset_s(buffer.as_mut_ptr(), 4, 0x41, 4) },
        0
    );
    assert_eq!(buffer, [0x41; 4]);
    let mut small = [0u8; 2];
    assert_eq!(
        unsafe { raven_libc_str::raw::memset_s(small.as_mut_ptr(), 2, 0x42, 3) },
        EINVAL
    );
    assert_eq!(small, [0x42; 2]);
}

unsafe fn test_alloc(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, 1).expect("layout");
    unsafe { std::alloc::alloc(layout) }
}

unsafe fn test_dealloc(pointer: *mut u8, size: usize) {
    let layout = Layout::from_size_align(size, 1).expect("layout");
    unsafe { std::alloc::dealloc(pointer, layout) };
}

#[test]
fn str_strdup_with_copies() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/strdup.rs
    let source = b"sshd\0";
    let duplicate = unsafe { raven_libc_str::raw::strdup_with(test_alloc, source.as_ptr()) };
    assert!(!duplicate.is_null());
    let copied = unsafe { std::slice::from_raw_parts(duplicate, source.len()) };
    assert_eq!(copied, source);
    unsafe { test_dealloc(duplicate, source.len()) };
    unsafe fn null_alloc(_size: usize) -> *mut u8 {
        core::ptr::null_mut()
    }
    let failed = unsafe { raven_libc_str::raw::strdup_with(null_alloc, source.as_ptr()) };
    assert!(failed.is_null());
}

#[test]
fn str_swab_swaps_pairs() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/swab.rs
    let from = [1u8, 2, 3, 4];
    let mut to = [0u8; 4];
    unsafe { raven_libc_str::raw::swab(from.as_ptr(), to.as_mut_ptr(), 4) };
    assert_eq!(to, [2, 1, 4, 3]);
    let mut odd = [0u8; 4];
    unsafe { raven_libc_str::raw::swab(from.as_ptr(), odd.as_mut_ptr(), 3) };
    assert_eq!(odd, [2, 1, 0, 0]);
}

// --- ABI constant modules on the login path ---------------------------------------

#[test]
fn abi_generated_misc_constants() {
    // source: libc/ravenlibc/crates/raven-libc-abi/src/generated/misc.rs
    assert_eq!(raven_libc_abi::misc::LC_ALL, 6);
    assert_eq!(raven_libc_abi::misc::WNOHANG, 1);
    assert_eq!(raven_libc_abi::misc::RTLD_LAZY, 1);
    assert_eq!(raven_libc_abi::misc::FD_SETSIZE, 1024);
}

#[test]
fn abi_generated_poll_constants() {
    // source: libc/ravenlibc/crates/raven-libc-abi/src/generated/poll.rs
    assert_eq!(raven_libc_abi::poll::POLLIN, 0x001);
    assert_eq!(raven_libc_abi::poll::POLLOUT, 0x004);
    assert_eq!(raven_libc_abi::poll::POLLERR, 0x008);
    assert_eq!(raven_libc_abi::poll::POLLHUP, 0x010);
    assert_eq!(raven_libc_abi::poll::POLLNVAL, 0x020);
}

#[test]
fn abi_generated_stat_constants() {
    // source: libc/ravenlibc/crates/raven-libc-abi/src/generated/stat.rs
    assert_eq!(raven_libc_abi::stat::S_IFMT, 0o170000);
    assert_eq!(raven_libc_abi::stat::S_IFREG, 0o100000);
    assert_eq!(raven_libc_abi::stat::S_IFDIR, 0o040000);
    assert_eq!(raven_libc_abi::stat::S_IRUSR, 0o400);
}

#[test]
fn abi_generated_time_constants() {
    // source: libc/ravenlibc/crates/raven-libc-abi/src/generated/time.rs
    assert_eq!(raven_libc_abi::time::CLOCK_REALTIME, 0);
    assert_eq!(raven_libc_abi::time::CLOCK_MONOTONIC, 1);
    assert_eq!(raven_libc_abi::time::CLOCK_BOOTTIME, 7);
    assert_eq!(raven_libc_abi::time::TIMER_ABSTIME, 1);
}

#[test]
fn abi_generated_mman_constants() {
    // source: libc/ravenlibc/crates/raven-libc-abi/src/generated/mman.rs
    assert_eq!(raven_libc_abi::mman::PROT_READ, 0x1);
    assert_eq!(raven_libc_abi::mman::PROT_WRITE, 0x2);
    assert_eq!(raven_libc_abi::mman::MAP_PRIVATE, 0x2);
    assert_eq!(raven_libc_abi::mman::MAP_ANONYMOUS, 0x20);
}

#[test]
fn abi_generated_sysconf_constants() {
    // source: libc/ravenlibc/crates/raven-libc-abi/src/generated/sysconf.rs
    assert_eq!(raven_libc_abi::sysconf::_SC_PAGESIZE, 30);
    assert_eq!(raven_libc_abi::sysconf::_SC_OPEN_MAX, 4);
    assert_eq!(raven_libc_abi::sysconf::_SC_CLK_TCK, 2);
    assert_eq!(raven_libc_abi::sysconf::_SC_NPROCESSORS_ONLN, 84);
}

// --- signal entry path -------------------------------------------------------------

#[test]
fn signal_raw_setjmp_x86_64_round_trip() {
    // source: libc/ravenlibc/crates/raven-libc-signal/src/raw/setjmp_x86_64.rs
    let mut buffer = JmpBuf::ZERO;
    let saved = raven_libc_signal::setjmp(&mut buffer);
    if saved == 0 {
        raven_libc_signal::longjmp(&buffer, 7);
    }
    assert_eq!(saved, 7);
}

#[test]
fn signal_raw_sigsetjmp_x86_64_mask_tail() {
    // source: libc/ravenlibc/crates/raven-libc-signal/src/raw/sigsetjmp_x86_64.rs
    let mut buffer = JmpBuf::ZERO;
    let mut mask = 0x1234_5678u64;
    assert_eq!(
        raven_libc_signal::sigjmp::sigsetjmp_with_mask(&mut buffer, 1, &mut mask),
        0
    );
    assert_eq!(
        buffer.bytes[raven_libc_signal::raw::JMP_SS_OFFSET >> 3],
        0x1234_5678
    );
    let mut live = 0u64;
    assert_eq!(
        raven_libc_signal::sigjmp::sigsetjmp_tail(&mut buffer, 1, &mut live),
        1
    );
    assert_eq!(live, 0x1234_5678);
}

// --- stdio descriptor backend ------------------------------------------------------

struct MemEnv {
    data: Mutex<Vec<u8>>,
    position: AtomicI32,
    closed: AtomicBool,
    close_count: AtomicI32,
}

impl MemEnv {
    fn new() -> Self {
        Self {
            data: Mutex::new(Vec::new()),
            position: AtomicI32::new(0),
            closed: AtomicBool::new(false),
            close_count: AtomicI32::new(0),
        }
    }
}

impl SysFile for MemEnv {
    fn read(&self, _fd: i32, buffer: &mut [u8]) -> Result<usize, raven_libc_abi::errno::Errno> {
        let data = self.data.lock().expect("lock");
        let position = self.position.load(Ordering::Relaxed).max(0) as usize;
        if position >= data.len() {
            return Ok(0);
        }
        let count = buffer.len().min(data.len() - position);
        buffer[..count].copy_from_slice(&data[position..position + count]);
        self.position
            .store((position + count) as i32, Ordering::Relaxed);
        Ok(count)
    }

    fn write(&self, _fd: i32, buffer: &[u8]) -> Result<usize, raven_libc_abi::errno::Errno> {
        let mut data = self.data.lock().expect("lock");
        data.extend_from_slice(buffer);
        self.position.store(data.len() as i32, Ordering::Relaxed);
        Ok(buffer.len())
    }

    fn write2(
        &self,
        _fd: i32,
        first: &[u8],
        second: &[u8],
    ) -> Result<usize, raven_libc_abi::errno::Errno> {
        let mut data = self.data.lock().expect("lock");
        data.extend_from_slice(first);
        data.extend_from_slice(second);
        self.position.store(data.len() as i32, Ordering::Relaxed);
        Ok(first.len() + second.len())
    }

    fn seek(
        &self,
        _fd: i32,
        offset: i64,
        _whence: i32,
    ) -> Result<i64, raven_libc_abi::errno::Errno> {
        self.position.store(offset as i32, Ordering::Relaxed);
        Ok(offset)
    }

    fn close(&self, _fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        self.closed.store(true, Ordering::Relaxed);
        self.close_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn open(
        &self,
        _path: &[u8],
        flags: i32,
        _mode: u32,
    ) -> Result<i32, raven_libc_abi::errno::Errno> {
        // O_WRONLY|O_RDWR|O_TRUNC starts a new file; O_RDONLY keeps the contents for the read half.
        if flags & (0o1 | 0o2 | 0o1000) != 0 {
            self.data.lock().expect("lock").clear();
        }
        self.position.store(0, Ordering::Relaxed);
        Ok(3)
    }

    fn isatty(&self, _fd: i32) -> bool {
        false
    }

    fn fstat(&self, _fd: i32) -> Result<StatLite, raven_libc_abi::errno::Errno> {
        Ok(StatLite {
            mode: 0o100000,
            blksize: 16,
            is_char_dev: false,
        })
    }

    fn getfl(&self, _fd: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        Ok(0)
    }

    fn setfl(&self, _fd: i32, _flags: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn unlink(&self, _path: &[u8]) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn rmdir(&self, _path: &[u8]) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn rename(&self, _old: &[u8], _new: &[u8]) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn pipe2(&self, _flags: i32) -> Result<(i32, i32), raven_libc_abi::errno::Errno> {
        Err(raven_libc_abi::errno::Errno(EINVAL as u16))
    }

    fn random_suffix(&self, out: &mut [u8; 6]) {
        out.copy_from_slice(b"ABCDEF");
    }
}

impl Lock for MemEnv {
    fn tid(&self) -> i32 {
        1
    }

    fn wait(&self, _word: &AtomicI32, _expected: i32) {}

    fn wake(&self, _word: &AtomicI32, _n: i32) {}

    fn threaded(&self) -> bool {
        false
    }
}

#[test]
fn stdio_backend_fd_read_write_round_trip() {
    // source: libc/ravenlibc/crates/raven-libc-stdio/src/backend_fd.rs
    let state: &'static State<MemEnv> = Box::leak(Box::new(State::new(MemEnv::new())));
    let writer = fopen(state, b"/login", b"w").expect("fopen w");
    let writer = file_from_ptr(writer);
    assert_eq!(fwrite(writer, b"ssh-login", 1, 9).expect("fwrite"), 9);
    assert!(fflush(state, Some(writer)).is_ok());
    fclose(writer).expect("fclose writer");

    let reader = fopen(state, b"/login", b"r").expect("fopen r");
    let reader = file_from_ptr(reader);
    let mut buffer = [0u8; 9];
    assert_eq!(fread(reader, &mut buffer, 1, 9).expect("fread"), 9);
    assert_eq!(&buffer, b"ssh-login");
    fclose(reader).expect("fclose reader");
}

#[test]
fn stdio_closeall_closes_nonstandard() {
    // source: libc/ravenlibc/crates/raven-libc-stdio/src/closeall.rs
    let state: &'static State<MemEnv> = Box::leak(Box::new(State::new(MemEnv::new())));
    let first = fopen(state, b"/one", b"w").expect("fopen one");
    let second = fopen(state, b"/two", b"w").expect("fopen two");
    assert!(!first.is_null() && !second.is_null());
    assert!(fcloseall(state).is_ok());
    assert_eq!(state.env().close_count.load(Ordering::Relaxed), 2);
}

// --- PTY allocation (SSH chain C9) -------------------------------------------------

struct FakePty {
    flags: i32,
    closed: Vec<i32>,
}

impl FakePty {
    fn new() -> Self {
        Self {
            flags: 0,
            closed: Vec::new(),
        }
    }
}

impl PtyOps for FakePty {
    fn posix_openpt(&mut self, flags: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        self.flags = flags;
        Ok(10)
    }

    fn grantpt(&mut self, _fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn unlockpt(&mut self, _fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn ptsname_r(
        &mut self,
        _fd: i32,
        buffer: &mut [u8],
    ) -> Result<usize, raven_libc_abi::errno::Errno> {
        let name = b"/dev/pts/7";
        buffer[..name.len()].copy_from_slice(name);
        Ok(name.len())
    }

    fn open(&mut self, _path: &[u8], _flags: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        Ok(11)
    }

    fn close(&mut self, fd: i32) {
        self.closed.push(fd);
    }

    fn tcsetattr(
        &mut self,
        _fd: i32,
        _termios: &raven_libc_compat::seams::Termios,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn set_winsize(
        &mut self,
        _fd: i32,
        _winsize: &raven_libc_compat::seams::Winsize,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn setsid(&mut self) -> Result<i32, raven_libc_abi::errno::Errno> {
        Ok(12)
    }

    fn set_ctty(&mut self, _fd: i32) -> Result<(), raven_libc_abi::errno::Errno> {
        Ok(())
    }

    fn dup2(&mut self, _a: i32, b: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
        Ok(b)
    }

    fn fork(&mut self) -> Result<ForkResult, raven_libc_abi::errno::Errno> {
        Err(raven_libc_abi::errno::Errno(38))
    }
}

#[test]
fn pty_shared_openpty_name_and_flags() {
    // source: libc/ravenlibc/crates/raven-libc-compat/src/pty/shared.rs
    let mut pty = FakePty::new();
    let mut name = [0u8; 16];
    let pair = openpty(&mut pty, Some(&mut name), None, None).expect("openpty");
    assert_eq!(pair, (10, 11));
    assert_eq!(pty.flags, 0o402);
    assert_eq!(&name[..10], b"/dev/pts/7");
    assert_eq!(name[10], 0);
    assert!(pty.closed.is_empty());
}

// --- TCP path MTU and SACK (C5-C7) -------------------------------------------------

#[test]
fn tcpip_pmtu_floor_and_headers() {
    // source: crates/raven-tcpip/src/pmtu.rs
    use raven_tcpip::{IpAddr, IpFamily, Ipv4Addr, Ipv6Addr, floor_for, mss_headers, mssopt};
    let v4 = IpAddr::V4(Ipv4Addr::new([192, 0, 2, 1]));
    let v6 = IpAddr::V6(Ipv6Addr::LOOPBACK);
    assert_eq!(floor_for(v4), 576);
    assert_eq!(floor_for(v6), 1280);
    assert_eq!(mss_headers(IpFamily::V4), 40);
    assert_eq!(mss_headers(IpFamily::V6), 60);
    assert_eq!(mssopt(1500, 0, 536, IpFamily::V4), 1460);
}

#[test]
fn tcpip_sack_option_bytes_round_trip() {
    // source: crates/raven-tcpip/src/sack.rs
    use raven_tcpip::{receiver_blocks, sack_option_bytes};
    let blocks = [(0x10u32, 0x20u32)];
    assert_eq!(
        sack_option_bytes(&blocks),
        [5, 10, 0, 0, 0, 0x10, 0, 0, 0, 0x20]
    );
    assert_eq!(
        receiver_blocks(&[(1, 2), (3, 4)], false, None),
        vec![(3, 4), (1, 2)]
    );
}

// --- login output helpers ----------------------------------------------------------

#[test]
fn humanize_expand_number_scaled() {
    // source: libc/ravenlibc/crates/raven-libc-compat/src/humanize/expand.rs
    use raven_libc_compat::humanize::expand_number;
    assert_eq!(expand_number(b"1K"), Ok(1024));
    assert_eq!(expand_number(b"3MB"), Ok(3 << 20));
    assert_eq!(expand_number(b"0x10"), Ok(16));
}

#[test]
fn humanize_fmt_scaled_unit() {
    // source: libc/ravenlibc/crates/raven-libc-compat/src/humanize/scaled.rs
    use raven_libc_compat::humanize::{FMT_SCALED_STRSIZE, fmt_scaled};
    let mut buffer = [0u8; FMT_SCALED_STRSIZE];
    fmt_scaled(1024, &mut buffer).expect("fmt_scaled");
    assert_eq!(&buffer[..4], b"1.0K");
}

// --- CJK terminal tables -----------------------------------------------------------

#[test]
fn iconv_cjk_big5_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/big5.rs
    assert_eq!(raven_libc_iconv_cjk::BIG5[0][0], 12288);
    assert_eq!(raven_libc_iconv_cjk::BIG5[0][1], 65292);
}

#[test]
fn iconv_cjk_gb18030_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/gb18030.rs
    assert_eq!(raven_libc_iconv_cjk::GB18030[0][0], 19970);
    assert_eq!(raven_libc_iconv_cjk::GB18030[0][1], 19972);
}

#[test]
fn iconv_cjk_gb18030utf_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/gb18030utf.rs
    assert_eq!(raven_libc_iconv_cjk::GB18030UTF[0][0], 128);
    assert_eq!(raven_libc_iconv_cjk::GB18030UTF[0][1], 36);
}

#[test]
fn iconv_cjk_hkscs_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/hkscs.rs
    assert_eq!(raven_libc_iconv_cjk::HKSCS[0], 17392);
}

#[test]
fn iconv_cjk_jis0208_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/jis0208.rs
    assert_eq!(raven_libc_iconv_cjk::JIS0208[0][0], 12288);
    assert_eq!(raven_libc_iconv_cjk::JIS0208[0][1], 12289);
}

#[test]
fn iconv_cjk_ksc_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/ksc.rs
    assert_eq!(raven_libc_iconv_cjk::KSC[0][0], 12288);
    assert_eq!(raven_libc_iconv_cjk::KSC[0][1], 12289);
}

#[test]
fn iconv_cjk_revjis_first_cell() {
    // source: libc/ravenlibc/crates/raven-libc-iconv-cjk/src/revjis.rs
    assert_eq!(raven_libc_iconv_cjk::REVJIS[0], 31);
    assert_eq!(raven_libc_iconv_cjk::REVJIS[1], 80);
}

// --- libm generic and fdlibm --------------------------------------------------------

#[test]
fn math_generic_sqrt_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/sqrt.rs
    assert_eq!(raven_libc_m::sqrt(0.0), 0.0);
    assert_eq!(raven_libc_m::sqrt(4.0), 2.0);
}

#[test]
fn math_generic_fabs() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/fabs.rs
    assert_eq!(raven_libc_m::fabs(-3.0), 3.0);
    assert_eq!(raven_libc_m::fabs(2.5), 2.5);
}

#[test]
fn math_generic_ceil() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/ceil.rs
    assert_eq!(raven_libc_m::ceil(1.0), 1.0);
    assert_eq!(raven_libc_m::ceil(1.2), 2.0);
}

#[test]
fn math_generic_fmin() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/fmin.rs
    assert_eq!(raven_libc_m::fmin(2.0, 3.0), 2.0);
    assert_eq!(raven_libc_m::fmin(3.0, 2.0), 2.0);
}

#[test]
fn math_generic_fmax() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/fmax.rs
    assert_eq!(raven_libc_m::fmax(2.0, 3.0), 3.0);
    assert_eq!(raven_libc_m::fmax(3.0, 2.0), 3.0);
}

#[test]
fn math_generic_fmod() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/fmod.rs
    assert_eq!(raven_libc_m::fmod(1.0, 3.0), 1.0);
    assert_eq!(raven_libc_m::fmod(5.0, 2.0), 1.0);
}

#[test]
fn math_generic_rint() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/rint.rs
    assert_eq!(raven_libc_m::rint(1.0), 1.0);
    assert_eq!(raven_libc_m::rint(2.5), 2.0);
}

#[test]
fn math_generic_fdim() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/fdim.rs
    assert_eq!(raven_libc_m::fdim(5.0, 3.0), 2.0);
    assert_eq!(raven_libc_m::fdim(2.0, 3.0), 0.0);
}

#[test]
fn math_fdlibm_sin_small() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/sin.rs
    assert_eq!(raven_libc_m::sin(1e-9), 1e-9);
    assert_eq!(raven_libc_m::sin(0.0), 0.0);
}

#[test]
fn math_fdlibm_exp2_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp2.rs
    assert_eq!(raven_libc_m::exp2(0.0), 1.0);
    assert_eq!(raven_libc_m::exp2(1.0), 2.0);
}

#[test]
fn math_fdlibm_cos_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/cos.rs
    assert_eq!(raven_libc_m::cos(0.0), 1.0);
}

#[test]
fn math_fdlibm_tan_small() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tan.rs
    assert_eq!(raven_libc_m::tan(1e-9), 1e-9);
    assert_eq!(raven_libc_m::tan(0.0), 0.0);
}

#[test]
fn math_fdlibm_exp_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp.rs
    assert_eq!(raven_libc_m::exp(0.0), 1.0);
}

#[test]
fn math_fdlibm_log_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log.rs
    assert_eq!(raven_libc_m::log(1.0), 0.0);
}

#[test]
fn math_fdlibm_acos_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/acos.rs
    assert_eq!(raven_libc_m::acos(1.0), 0.0);
}

#[test]
fn math_fdlibm_acosf_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/acosf.rs
    assert_eq!(raven_libc_m::acosf(1.0), 0.0);
}

#[test]
fn math_fdlibm_acoshf_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/acoshf.rs
    assert_eq!(raven_libc_m::acoshf(1.0), 0.0);
}

#[test]
fn math_fdlibm_asin_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/asin.rs
    assert_eq!(raven_libc_m::asin(0.0), 0.0);
}

#[test]
fn math_fdlibm_asinf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/asinf.rs
    assert_eq!(raven_libc_m::asinf(0.0), 0.0);
}

#[test]
fn math_fdlibm_asinhf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/asinhf.rs
    assert_eq!(raven_libc_m::asinhf(0.0), 0.0);
}

#[test]
fn math_fdlibm_atan2f_zero_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/atan2f.rs
    assert_eq!(raven_libc_m::atan2f(0.0, 1.0), 0.0);
}

#[test]
fn math_fdlibm_atanf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/atanf.rs
    assert_eq!(raven_libc_m::atanf(0.0), 0.0);
}

#[test]
fn math_fdlibm_atanhf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/atanhf.rs
    assert_eq!(raven_libc_m::atanhf(0.0), 0.0);
}

#[test]
fn math_fdlibm_cosf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/cosf.rs
    assert_eq!(raven_libc_m::cosf(0.0), 1.0);
}

#[test]
fn math_fdlibm_cosh_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/cosh.rs
    assert_eq!(raven_libc_m::cosh(0.0), 1.0);
}

#[test]
fn math_fdlibm_coshf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/coshf.rs
    assert_eq!(raven_libc_m::coshf(0.0), 1.0);
}

#[test]
fn math_fdlibm_erf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/erf.rs
    assert_eq!(raven_libc_m::erf(0.0), 0.0);
}

#[test]
fn math_fdlibm_erff_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/erff.rs
    assert_eq!(raven_libc_m::erff(0.0), 0.0);
}

#[test]
fn math_fdlibm_exp10_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp10.rs
    assert_eq!(raven_libc_m::exp10(0.0), 1.0);
}

#[test]
fn math_fdlibm_exp2f_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/exp2f.rs
    assert_eq!(raven_libc_m::exp2f(0.0), 1.0);
}

#[test]
fn math_fdlibm_expf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/expf.rs
    assert_eq!(raven_libc_m::expf(0.0), 1.0);
}

#[test]
fn math_fdlibm_expm1f_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/expm1f.rs
    assert_eq!(raven_libc_m::expm1f(0.0), 0.0);
}

#[test]
fn math_generic_fma_basic() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/generic/fma.rs
    assert_eq!(raven_libc_m::fma(0.0, 0.0, 5.0), 5.0);
}

#[test]
fn math_fdlibm_log10f_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log10f.rs
    assert_eq!(raven_libc_m::log10f(1.0), 0.0);
}

#[test]
fn math_fdlibm_log1pf_zero() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log1pf.rs
    assert_eq!(raven_libc_m::log1pf(0.0), 0.0);
}

#[test]
fn math_fdlibm_log2_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log2.rs
    assert_eq!(raven_libc_m::log2(1.0), 0.0);
}

#[test]
fn math_fdlibm_log2f_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/log2f.rs
    assert_eq!(raven_libc_m::log2f(1.0), 0.0);
}

#[test]
fn math_fdlibm_logf_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/logf.rs
    assert_eq!(raven_libc_m::logf(1.0), 0.0);
}

#[test]
fn math_fdlibm_tgammaf_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tgammaf.rs
    assert_eq!(raven_libc_m::tgammaf(1.0), 1.0);
}

#[test]
fn math_fdlibm_powf_one() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/powf.rs
    assert_eq!(raven_libc_m::powf(1.0, 3.0), 1.0);
    assert_eq!(raven_libc_m::powf(2.0, 3.0), 8.0);
}

// --- K507: next 25 INDIRECT rows (transcendental/Bessel/NAN family) -----------------

fn close64(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-9
}

fn close32(actual: f32, expected: f32) -> bool {
    (actual - expected).abs() < 1e-5
}

#[test]
fn math_coremath_cbrt() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/coremath/cbrt.rs
    assert_eq!(raven_libc_m::cbrt(27.0), 3.0);
    assert_eq!(raven_libc_m::cbrt(-8.0), -2.0);
}

#[test]
fn math_fdlibm_expo2() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/expo2.rs
    let value = raven_libc_m::sinh(710.0);
    assert!(value.is_finite());
    assert!(value > 0.0);
}

#[test]
fn math_fdlibm_k_expo2() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_expo2.rs
    let value = raven_libc_m::cosh(710.0);
    assert!(value.is_finite());
    assert!(value > 0.0);
}

#[test]
fn math_fdlibm_k_expo2f() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_expo2f.rs
    let value = raven_libc_m::coshf(89.0);
    assert!(value.is_finite());
    assert!(value > 0.0);
}

#[test]
fn math_fdlibm_j0() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/j0.rs
    assert_eq!(raven_libc_m::j0(0.0), 1.0);
    assert!(close64(raven_libc_m::j0(1.0), 0.765_197_686_557_966_6));
}

#[test]
fn math_fdlibm_j0f() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/j0f.rs
    assert_eq!(raven_libc_m::j0f(0.0), 1.0);
    assert!(close32(raven_libc_m::j0f(1.0), 0.765_197_7));
}

#[test]
fn math_fdlibm_j1() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/j1.rs
    assert_eq!(raven_libc_m::j1(0.0), 0.0);
    assert!(close64(raven_libc_m::j1(1.0), 0.440_050_585_744_933_55));
}

#[test]
fn math_fdlibm_jn() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/jn.rs
    assert_eq!(raven_libc_m::jn(0, 0.0), 1.0);
    assert!(close64(raven_libc_m::jn(1, 1.0), 0.440_050_585_744_933_55));
    // jn(2, 3) is the first order that runs jn's own forward recurrence.
    assert!(close64(raven_libc_m::jn(2, 3.0), 0.486_091_260_585_891));
}

#[test]
fn math_fdlibm_jnf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/jnf.rs
    assert_eq!(raven_libc_m::jnf(0, 0.0), 1.0);
    assert!(close32(raven_libc_m::jnf(1, 1.0), 0.440_050_57));
    assert!(close32(raven_libc_m::jnf(2, 3.0), 0.486_091_26));
}

#[test]
fn math_fdlibm_k_cos() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_cos.rs
    // cos(0.5) has |x| <= pi/4 and dispatches straight into k_cos.
    assert!(close64(raven_libc_m::cos(0.5), 0.877_582_561_890_372_8));
}

#[test]
fn math_fdlibm_k_cosf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_cosf.rs
    // cosf(0.5) has |x| <= pi/4 and dispatches straight into k_cosf.
    assert!(close32(raven_libc_m::cosf(0.5), 0.877_582_55));
}

#[test]
fn math_fdlibm_k_sin() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_sin.rs
    // sin(0.5) has |x| <= pi/4 and dispatches straight into k_sin.
    assert!(close64(raven_libc_m::sin(0.5), 0.479_425_538_604_203));
}

#[test]
fn math_fdlibm_k_sinf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_sinf.rs
    // sinf(0.5) has |x| <= pi/4 and dispatches straight into k_sinf.
    assert!(close32(raven_libc_m::sinf(0.5), 0.479_425_55));
}

#[test]
fn math_fdlibm_k_tan() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_tan.rs
    assert!(close64(raven_libc_m::tan(1.0), 1.557_407_724_654_902_3));
}

#[test]
fn math_fdlibm_k_tanf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/k_tanf.rs
    assert!(close32(raven_libc_m::tanf(1.0), 1.557_407_7));
}

#[test]
fn math_portable_modf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/portable/modf.rs
    // `raven_libc_m::modf` routes to portable::modf; fdlibm/modf.rs is an
    // unreferenced duplicate of this file.
    let (fraction, integral) = raven_libc_m::modf(3.5);
    assert_eq!(fraction, 0.5);
    assert_eq!(integral, 3.0);
}

#[test]
fn math_fdlibm_rem_pio2_large() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/rem_pio2_large.rs
    // sin(1e20) needs Payne-Hanek reduction through rem_pio2_large.
    assert!(close64(raven_libc_m::sin(1.0e20), -0.645_251_285_265_780_8));
}

#[test]
fn math_fdlibm_rem_pio2f() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/rem_pio2f.rs
    assert!(close32(raven_libc_m::sinf(1.0e20), 0.656_576_7));
}

#[test]
fn math_fdlibm_sinf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/sinf.rs
    assert_eq!(raven_libc_m::sinf(0.0), 0.0);
    assert!(close32(raven_libc_m::sinf(1.0), 0.841_470_96));
}

#[test]
fn math_fdlibm_sinh() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/sinh.rs
    assert_eq!(raven_libc_m::sinh(0.0), 0.0);
    assert!(close64(raven_libc_m::sinh(1.0), 1.175_201_193_643_801_4));
}

#[test]
fn math_fdlibm_sinhf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/sinhf.rs
    assert_eq!(raven_libc_m::sinhf(0.0), 0.0);
    assert!(close32(raven_libc_m::sinhf(1.0), 1.175_201_2));
}

#[test]
fn math_fdlibm_tanf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tanf.rs
    assert_eq!(raven_libc_m::tanf(0.0), 0.0);
    assert!(close32(raven_libc_m::tanf(1.0), 1.557_407_7));
}

#[test]
fn math_fdlibm_tanh() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tanh.rs
    assert_eq!(raven_libc_m::tanh(0.0), 0.0);
    assert!(close64(raven_libc_m::tanh(1.0), 0.761_594_155_955_764_9));
}

#[test]
fn math_fdlibm_tanhf() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/tanhf.rs
    assert_eq!(raven_libc_m::tanhf(0.0), 0.0);
    assert!(close32(raven_libc_m::tanhf(1.0), 0.761_594_2));
}

#[test]
fn math_fp_nan() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fp/nan.rs
    assert!(raven_libc_m::nan(b"0x1234").is_nan());
    assert!(raven_libc_m::nan(b"").is_nan());
    assert_ne!(
        raven_libc_m::nan(b"1234").to_bits(),
        raven_libc_m::nan(b"").to_bits()
    );
}

// --- K519: the last 19 INDIRECT libc rows ------------------------------------------

/// Source-inclusion test of the unreferenced `support/env.rs` (the file has no public path).
/// Items the libm crate uses elsewhere are dead in this isolated copy.
#[allow(dead_code)]
#[path = "../../../../libc/ravenlibc/crates/raven-libc-m/src/support/env.rs"]
mod k519_support_env;

/// The generic block bodies are not compiled on x86_64 without the `size` feature; include the
/// exact source so the host test can exercise it without changing the product build.
#[path = "../../../../libc/ravenlibc/crates/raven-libc-str/src/raw/mem/impls.rs"]
mod k519_mem_impls;

/// The included `raw/strcoll.rs` resolves `super::{strcmp,strlen,common}` here.
mod strcmp {
    pub use raven_libc_str::raw::strcmp::strcmp;
}
mod strlen {
    pub use raven_libc_str::raw::strlen::strlen;
}
mod common {
    /// # Safety
    /// `p..p+len` is readable for the returned lifetime.
    pub unsafe fn bytes<'a>(p: *const u8, len: usize) -> &'a [u8] {
        unsafe { core::slice::from_raw_parts(p, len) }
    }
    /// # Safety
    /// `p..p+len` is writable for the returned lifetime.
    pub unsafe fn bytes_mut<'a>(p: *mut u8, len: usize) -> &'a mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(p, len) }
    }
}
/// The exact source of the unreferenced `raw/strcoll.rs`; `crate::cstr` comes from `lib.rs`.
#[path = "../../../../libc/ravenlibc/crates/raven-libc-str/src/raw/strcoll.rs"]
mod k519_strcoll;

#[test]
fn util_dtoa_bigint_misc() {
    // source: libc/ravenlibc/crates/raven-libc-dtoa/src/bigint/misc.rs
    let mut ctx = raven_libc_dtoa::DtoaCtx::new();
    let mut out = [0u8; 800];
    let d = raven_libc_dtoa::dtoa(
        &mut ctx,
        f64::MAX,
        raven_libc_dtoa::DigitMode::Shortest,
        0,
        raven_libc_dtoa::Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(&out[..d.len], b"17976931348623157");
    assert_eq!(d.decpt, 309);
}

#[test]
fn util_fts_dirent_dirstream() {
    // source: libc/ravenlibc/crates/raven-libc-fts/src/dirent/dirstream.rs
    use raven_libc_fts::DirStream;
    use raven_libc_fts::fake::FakeFs;
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    assert_eq!(dir.telldir(), 0);
    let first = dir.readdir(&mut fs).unwrap().unwrap();
    assert_eq!(first.name, b".");
    assert_eq!(dir.telldir(), first.off);
    assert!(dir.telldir() > 0);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn util_fts_ftw() {
    // source: libc/ravenlibc/crates/raven-libc-fts/src/ftw.rs
    use raven_libc_abi::generated::structs::Stat;
    use raven_libc_fts::fake::FakeFs;
    use raven_libc_fts::{FTW_D, FTW_F, ftw};
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.file("r/a", 1);
    fs.mkdir("r/sub", 0o755);
    fs.file("r/sub/x", 2);
    let mut seen: Vec<(Vec<u8>, i32)> = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, flag: i32| {
        seen.push((path.to_vec(), flag));
        0
    };
    assert_eq!(ftw(&mut fs, b"r", &mut callback, 1).unwrap(), 0);
    assert!(seen.contains(&(b"r".to_vec(), FTW_D)));
    assert!(seen.contains(&(b"r/a".to_vec(), FTW_F)));
    assert!(seen.contains(&(b"r/sub/x".to_vec(), FTW_F)));
}

#[test]
fn util_fts_nftw() {
    // source: libc/ravenlibc/crates/raven-libc-fts/src/nftw.rs
    use raven_libc_abi::generated::structs::Stat;
    use raven_libc_fts::fake::FakeFs;
    use raven_libc_fts::{FTW_D, FTW_F, Ftw, nftw};
    let mut fs = FakeFs::new();
    fs.mkdir("r", 0o755);
    fs.file("r/a", 1);
    fs.mkdir("r/sub", 0o755);
    fs.file("r/sub/x", 2);
    let mut seen: Vec<(Vec<u8>, i32, i64)> = Vec::new();
    let mut callback = |path: &[u8], _stat: &Stat, flag: i32, ftw: &Ftw| {
        seen.push((path.to_vec(), flag, ftw.level));
        0
    };
    assert_eq!(nftw(&mut fs, b"r", &mut callback, 1, 0).unwrap(), 0);
    assert!(seen.contains(&(b"r".to_vec(), FTW_D, 0)));
    assert!(seen.contains(&(b"r/a".to_vec(), FTW_F, 1)));
    assert!(seen.contains(&(b"r/sub/x".to_vec(), FTW_F, 2)));
}

#[test]
fn util_locale_gettext() {
    // source: libc/ravenlibc/crates/raven-libc-locale/src/gettext/mod.rs
    use std::collections::BTreeMap;

    use raven_libc_locale::gettext::{FileSource, Gettext};
    use raven_libc_locale::locale::{EnvSource, LC_MESSAGES, LocaleState};

    struct Fs(BTreeMap<Vec<u8>, Vec<u8>>);
    impl FileSource for Fs {
        fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>> {
            self.0.get(path).filter(|data| data.len() <= max).cloned()
        }
    }
    struct Env(&'static [(&'static [u8], &'static [u8])]);
    impl EnvSource for Env {
        fn get(&self, name: &[u8]) -> Option<&[u8]> {
            self.0
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
        }
    }
    fn mo(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
        let n = entries.len();
        let orig = 20usize;
        let trans = orig + 8 * n;
        let strings = trans + 8 * n;
        let mut data = vec![0u8; strings];
        let put = |data: &mut Vec<u8>, at: usize, value: u32| {
            if let Some(slot) = data.get_mut(at..at + 4) {
                slot.copy_from_slice(&value.to_le_bytes());
            }
        };
        put(&mut data, 0, 0x9504_12de);
        put(&mut data, 8, n as u32);
        put(&mut data, 12, orig as u32);
        put(&mut data, 16, trans as u32);
        for (index, (key, value)) in entries.iter().enumerate() {
            let key_at = data.len();
            data.extend_from_slice(key);
            data.push(0);
            let value_at = data.len();
            data.extend_from_slice(value);
            data.push(0);
            put(&mut data, orig + index * 8, key.len() as u32);
            put(&mut data, orig + index * 8 + 4, key_at as u32);
            put(&mut data, trans + index * 8, value.len() as u32);
            put(&mut data, trans + index * 8 + 4, value_at as u32);
        }
        data
    }

    let data = mo(&[(b"file", b"fichier\0fichiers")]);
    let mut files = BTreeMap::new();
    files.insert(
        b"/usr/share/locale/en_US.UTF-8/LC_MESSAGES/messages.mo".to_vec(),
        data,
    );
    let fs = Fs(files);
    let env = Env(&[]);
    let mut state = LocaleState::new();
    let _ = state.setlocale(LC_MESSAGES, Some(b"en_US.UTF-8"), &env);
    let mut gettext = Gettext::new();
    let singular = gettext.dcngettext(
        &fs,
        &env,
        None,
        b"file",
        b"files",
        1,
        LC_MESSAGES,
        &state.global(),
        state.table(),
    );
    assert_eq!(singular, b"fichier");
    let plural = gettext.dcngettext(
        &fs,
        &env,
        None,
        b"file",
        b"files",
        2,
        LC_MESSAGES,
        &state.global(),
        state.table(),
    );
    assert_eq!(plural, b"fichiers");
}

#[test]
fn math_support_env_status() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/support/env.rs
    let mut status = k519_support_env::Status::OK;
    assert!(!status.inexact());
    status.set_inexact(true);
    assert!(status.inexact());
    status.set_inexact(false);
    assert!(!status.inexact());
    let mut under = k519_support_env::Status::OK;
    under.set_underflow(true);
    assert!(under.underflow());
    let result = k519_support_env::FpResult::ok(2.5);
    assert_eq!(result.val, 2.5);
    assert!(!result.status.inexact());
}

#[test]
fn math_support_narrowing_div_fmod() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/support/int_traits/narrowing_div.rs
    // The large-remainder path uses `checked_narrowing_div_rem` on the u128 double-wide type.
    assert_eq!(raven_libc_m::fmod(1.0e300, 7.0), 1.0);
}

#[test]
fn math_support_macros_hf64() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/support/macros.rs
    // `cbrt` evaluates `hf64!` constants; a broken `hf64!` changes the result.
    assert_eq!(raven_libc_m::cbrt(27.0), 3.0);
}

struct K519Arena {
    base: usize,
    len: usize,
    used: core::cell::Cell<usize>,
}

impl K519Arena {
    fn new(len: usize) -> Self {
        let layout = Layout::from_size_align(len, 4096).unwrap();
        let base = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!base.is_null());
        Self {
            base: base as usize,
            len,
            used: core::cell::Cell::new(0),
        }
    }
}

impl raven_libc_malloc::PageProvider for K519Arena {
    fn page_size(&self) -> usize {
        4096
    }
    fn map_pages(&self, len: usize, align: usize, _prot: u8) -> Option<usize> {
        let used = self.used.get();
        let aligned = (self.base + used + align - 1) & !(align - 1);
        let start = aligned - self.base;
        if start + len > self.len {
            return None;
        }
        self.used.set(start + len);
        Some(aligned)
    }
    fn map_exact(&self, hint: usize, len: usize) -> Option<usize> {
        if hint == self.base + self.used.get() && self.used.get() + len <= self.len {
            self.used.set(self.used.get() + len);
            return Some(hint);
        }
        None
    }
    fn unmap(&self, _addr: usize, _len: usize) -> bool {
        true
    }
    fn protect(&self, _addr: usize, _len: usize, _prot: u8) -> bool {
        true
    }
}

struct K519Locks;
impl raven_libc_malloc::Lock for K519Locks {
    fn lock(&self, _index: usize) {}
    fn unlock(&self, _index: usize) {}
    fn reset(&self, _index: usize) {}
}

struct K519Entropy;
impl raven_libc_malloc::Entropy for K519Entropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        for (index, byte) in buf.iter_mut().enumerate() {
            *byte = (index as u8) | 1;
        }
        true
    }
}

struct K519Platform;
impl raven_libc_malloc::Platform for K519Platform {
    fn thread_id(&self) -> usize {
        0
    }
    fn set_errno(&self, _errno: i32) {}
    fn write_stderr(&self, _bytes: &[u8]) {}
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"k519"
    }
    fn process_id(&self) -> u32 {
        1
    }
}

#[test]
fn util_malloc_hardened() {
    // source: libc/ravenlibc/crates/raven-libc-malloc/src/hardened/mod.rs
    use raven_libc_malloc::{Hardened, Heap};
    let heap = Hardened::new(
        K519Arena::new(1 << 20),
        K519Locks,
        K519Entropy,
        K519Platform,
        b"",
    );
    let first = heap.malloc(16);
    assert_ne!(first, 0);
    let second = heap.malloc(32);
    assert_ne!(second, 0);
    assert_ne!(first, second);
    heap.free(second);
    heap.free(first);
}

#[test]
fn signal_raw_setjmp_aarch64_abi() {
    // source: libc/ravenlibc/crates/raven-libc-signal/src/raw/setjmp_aarch64.rs
    // AArch64-only assembly: the host test checks the musl slot contract (H-20).
    let source = include_str!(
        "../../../../libc/ravenlibc/crates/raven-libc-signal/src/raw/setjmp_aarch64.rs"
    );
    assert!(source.contains("stp x19, x20, [x0,#0]"));
    assert!(source.contains("stp x29, x30, [x0,#80]"));
    assert!(source.contains("str x2, [x0,#104]"));
    assert!(source.contains("stp d8, d9, [x0,#112]"));
    assert!(source.contains("ldp x19, x20, [x0,#0]"));
    assert!(source.contains("ldr x2, [x0,#104]"));
}

#[test]
fn signal_raw_setjmp_riscv64_abi() {
    // source: libc/ravenlibc/crates/raven-libc-signal/src/raw/setjmp_riscv64.rs
    let source = include_str!(
        "../../../../libc/ravenlibc/crates/raven-libc-signal/src/raw/setjmp_riscv64.rs"
    );
    assert!(source.contains("sd s0, 0(a0)"));
    assert!(source.contains("sd s11, 88(a0)"));
    assert!(source.contains("sd sp, 96(a0)"));
    assert!(source.contains("sd ra, 104(a0)"));
    assert!(source.contains("fsd fs0, 112(a0)"));
    assert!(source.contains("ld s0, 0(a0)"));
    assert!(source.contains("ld ra, 104(a0)"));
}

#[test]
fn signal_raw_sigsetjmp_aarch64_abi() {
    // source: libc/ravenlibc/crates/raven-libc-signal/src/raw/sigsetjmp_aarch64.rs
    let source = include_str!(
        "../../../../libc/ravenlibc/crates/raven-libc-signal/src/raw/sigsetjmp_aarch64.rs"
    );
    assert!(source.contains("str x30, [x0,#256]"));
    assert!(source.contains("str x19, [x0,#272]"));
    assert!(source.contains("str x2, [x0,#280]"));
    assert!(source.contains("ldr x3, [x0,#264]"));
}

#[test]
fn signal_raw_sigsetjmp_riscv64_abi() {
    // source: libc/ravenlibc/crates/raven-libc-signal/src/raw/sigsetjmp_riscv64.rs
    let source = include_str!(
        "../../../../libc/ravenlibc/crates/raven-libc-signal/src/raw/sigsetjmp_riscv64.rs"
    );
    assert!(source.contains("sd ra, 256(a0)"));
    assert!(source.contains("sd s0, 272(a0)"));
    assert!(source.contains("sd a2, 280(a0)"));
    assert!(source.contains("ld a3, 264(a0)"));
}

#[test]
fn util_str_mem_impls() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/mem/impls.rs
    // The x86_64 build uses the arch block; this exercises the generic donor bodies directly.
    let mut dst = [0u8; 5];
    unsafe {
        k519_mem_impls::copy_forward(dst.as_mut_ptr(), b"hello".as_ptr(), 5);
    }
    assert_eq!(&dst, b"hello");
    let mut overlap = *b"abcdef";
    unsafe {
        k519_mem_impls::copy_backward(overlap.as_mut_ptr().add(1), overlap.as_ptr(), 5);
    }
    assert_eq!(&overlap, b"aabcde");
    let mut set = [0u8; 4];
    unsafe {
        k519_mem_impls::set_bytes(set.as_mut_ptr(), b'x', set.len());
    }
    assert_eq!(&set, b"xxxx");
    assert!(unsafe { k519_mem_impls::compare_bytes(b"abc".as_ptr(), b"abd".as_ptr(), 3) } < 0);
    assert_eq!(
        unsafe { k519_mem_impls::c_string_length(c"abc".as_ptr()) },
        3
    );
}

#[test]
fn util_str_raw_strcoll() {
    // source: libc/ravenlibc/crates/raven-libc-str/src/raw/strcoll.rs
    // The file is unreferenced; this includes its exact source with the same helper contracts.
    assert!(unsafe { k519_strcoll::strcoll(c"abc".as_ptr().cast(), c"abd".as_ptr().cast()) } < 0);
    let mut dst = [0u8; 8];
    let copied =
        unsafe { k519_strcoll::strxfrm(dst.as_mut_ptr(), c"abc".as_ptr().cast(), dst.len()) };
    assert_eq!(copied, 3);
    assert_eq!(&dst[..4], b"abc\0");
}

#[test]
fn util_ld_self_object() {
    // source: libc/ravenlibc/ld/src/self_object.rs
    use raven_libc_elf::{DigestError, SliceMem};
    let mem = SliceMem::new(0, &[]);
    assert_eq!(
        raven_libc_rtld::self_object::digest_self(&mem, 0, 65, 0).unwrap_err(),
        DigestError::BadDynamic
    );
    assert_eq!(
        raven_libc_rtld::self_object::digest_self(&mem, 0, 0, 0).unwrap_err(),
        DigestError::NotDynamic
    );
}

/// The unreferenced `fdlibm/modf.rs` duplicate; the exact source is tested directly.
/// The file is written for the libm crate's lint set, so these two style lints are local noise.
#[allow(clippy::needless_return, clippy::needless_late_init)]
#[path = "../../../../libc/ravenlibc/crates/raven-libc-m/src/fdlibm/modf.rs"]
mod k519_fdlibm_modf;

#[test]
fn math_fdlibm_modf_duplicate() {
    // source: libc/ravenlibc/crates/raven-libc-m/src/fdlibm/modf.rs
    let (fraction, integral) = k519_fdlibm_modf::modf(3.5);
    assert_eq!(fraction, 0.5);
    assert_eq!(integral, 3.0);
    let (negative_fraction, negative_integral) = k519_fdlibm_modf::modf(-0.75);
    assert_eq!(negative_fraction, -0.75);
    assert!(negative_integral == 0.0 && negative_integral.is_sign_negative());
}
