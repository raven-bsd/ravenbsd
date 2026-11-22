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
