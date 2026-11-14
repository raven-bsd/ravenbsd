// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host-run h8: the pure name-service codec of C64 (bytes exchange, `inet_*`, `gai_strerror`,
//! `hstrerror`, `dn_expand`, `dn_skipname`, `ns_get/put`, `ns_skiprr`, `res_init`).
//!
//! Nothing here makes a Raven system call: the checks are the C51 codec through the shell's
//! pointer conversions. The socket and `Env`/`DnsIo` exports are deferred with C63 and are not
//! called. The exit code is the index of the first failed check.

#![no_std]
#![no_main]

use core::arch::asm;
use core::ffi::{c_char, c_int, c_void};

unsafe fn write(bytes: &[u8]) {
    let result: isize;
    // SAFETY: Linux write(2) with a valid buffer.
    unsafe {
        asm!("syscall", in("rax") 1usize, in("rdi") 1usize, in("rsi") bytes.as_ptr(), in("rdx") bytes.len(), lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    let _ = result;
}

unsafe fn exit(code: i32) -> ! {
    // SAFETY: Linux exit_group(2) never returns.
    unsafe { asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn)) }
}

fn fail(index: i32) -> ! {
    let mut text = *b"H8 FAIL 0\n";
    if let Some(byte) = text.get_mut(8) {
        *byte = b'0' + u8::try_from(index).unwrap_or(0);
    }
    // SAFETY: fixed local buffer and exit_group never returns.
    unsafe {
        write(&text);
        exit(index)
    }
}

fn bytes_equal(left: *const c_char, right: &[u8]) -> bool {
    let mut index = 0usize;
    loop {
        // SAFETY: both sides are the test's own static NUL-terminated byte arrays.
        let left_byte = unsafe { *left.cast::<u8>().add(index) };
        let right_byte = right.get(index).copied().unwrap_or(0);
        if left_byte != right_byte {
            return false;
        }
        if left_byte == 0 {
            return true;
        }
        index = index.wrapping_add(1);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut word16 = [0u8; 2];
    let mut word32 = [0u8; 4];
    let mut address = [0u8; 16];
    let mut name = [0u8; 64];
    // SAFETY: every pointer below follows its C contract.
    unsafe {
        if htons(0x1234) != 0x3412 || ntohs(0x3412) != 0x1234 {
            fail(1);
        }
        if htonl(0x1234_5678) != 0x7856_3412 || ntohl(0x7856_3412) != 0x1234_5678 {
            fail(2);
        }
        let mut literal = [0u8; 16];
        if inet_aton(b"1.2.3\0".as_ptr().cast(), literal.as_mut_ptr().cast()) != 1 {
            fail(3);
        }
        if literal.get(..4) != Some([1u8, 2, 0, 3].as_slice()) {
            fail(4);
        }
        if inet_addr(b"256.1.1.1\0".as_ptr().cast()) != 0xffff_ffff {
            fail(5);
        }
        if inet_pton(AF_INET, b"010.0.0.1\0".as_ptr().cast(), address.as_mut_ptr().cast()) != 0 {
            fail(6);
        }
        if inet_pton(AF_INET6, b"::ffff:1.2.3.4\0".as_ptr().cast(), address.as_mut_ptr().cast())
            != 1
        {
            fail(7);
        }
        if address != [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 1, 2, 3, 4] {
            fail(8);
        }
        if inet_pton(AF_INET6, b"1:2:3:4:5:6:7:8:9\0".as_ptr().cast(), address.as_mut_ptr().cast())
            != 0
        {
            fail(9);
        }
        if inet_network(b"127.1\0".as_ptr().cast()) != 0x7f00_0001 {
            fail(12);
        }
        if inet_makeaddr(0x7f, 1).s_addr != 0x7f00_0001
            || inet_lnaof(InAddr { s_addr: 0x7f00_0002 }) != 2
            || inet_netof(InAddr { s_addr: 0x7f00_0002 }) != 0x7f
        {
            fail(13);
        }
        if !bytes_equal(gai_strerror(-2), b"Name does not resolve") {
            fail(40);
        }
        if !bytes_equal(gai_strerror(7), b"Unknown error") {
            fail(41);
        }
        if !bytes_equal(hstrerror(1), b"Host not found") {
            fail(42);
        }
        if !bytes_equal(hstrerror(0), b"Unknown error") {
            fail(43);
        }
        if !in6addr_any.s6_addr.iter().all(|byte| *byte == 0)
            || in6addr_loopback.s6_addr.get(15) != Some(&1)
        {
            fail(16);
        }
        let location = __h_errno_location();
        if location.is_null() || *location != 0 {
            fail(17);
        }
        if res_init() != 0 {
            fail(18);
        }
        let compressed = *b"\x03www\x00\xc0\x00";
        let consumed = dn_expand(
            compressed.as_ptr(),
            compressed.as_ptr().add(7),
            compressed.as_ptr().add(5),
            name.as_mut_ptr().cast(),
            64,
        );
        if consumed != 2 || !bytes_equal(name.as_ptr().cast(), b"www") {
            fail(19);
        }
        let uncompressed = *b"\x03www\x00";
        if dn_skipname(uncompressed.as_ptr(), uncompressed.as_ptr().add(5)) != 5
            || dn_skipname(compressed.as_ptr().add(5), compressed.as_ptr().add(7)) != 2
        {
            fail(20);
        }
        let be16 = [0x12u8, 0x34];
        if ns_get16(be16.as_ptr()) != 0x1234 {
            fail(21);
        }
        if ns_put16(0xabcd, word16.as_mut_ptr()) != 1 || word16 != [0xab, 0xcd] {
            fail(22);
        }
        let be32 = [0x12u8, 0x34, 0x56, 0x78];
        if ns_get32(be32.as_ptr()) != 0x1234_5678 {
            fail(23);
        }
        if ns_put32(0x0102_0304, word32.as_mut_ptr()) != 1 || word32 != [1, 2, 3, 4] {
            fail(24);
        }
        let question = *b"\x00\x01\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x03www\x00\x00\x01\x00\x01";
        if ns_skiprr(
            question.as_ptr().add(12),
            question.as_ptr().add(21),
            0,
            1,
        ) != 9
        {
            fail(25);
        }
    }
    // SAFETY: fixed success text and exit_group never returns.
    unsafe {
        write(b"H8 OK\n");
        exit(0)
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InAddr {
    s_addr: u32,
}

const AF_INET: c_int = 2;
const AF_INET6: c_int = 10;

unsafe extern "C" {
    fn htons(value: u16) -> u16;
    fn htonl(value: u32) -> u32;
    fn ntohs(value: u16) -> u16;
    fn ntohl(value: u32) -> u32;
    fn inet_aton(text: *const c_char, address: *mut c_void) -> c_int;
    fn inet_addr(text: *const c_char) -> u32;
    fn inet_network(text: *const c_char) -> u32;
    fn inet_makeaddr(network: u32, host: u32) -> InAddr;
    fn inet_lnaof(address: InAddr) -> u32;
    fn inet_netof(address: InAddr) -> u32;
    fn inet_pton(af: c_int, text: *const c_char, dst: *mut c_void) -> c_int;
    fn gai_strerror(code: c_int) -> *const c_char;
    fn hstrerror(code: c_int) -> *const c_char;
    fn __h_errno_location() -> *mut c_int;
    fn res_init() -> c_int;
    fn dn_expand(
        msg: *const u8,
        eom: *const u8,
        comp_dn: *const u8,
        exp_dn: *mut c_char,
        length: c_int,
    ) -> c_int;
    fn dn_skipname(comp_dn: *const u8, eom: *const u8) -> c_int;
    fn ns_get16(cp: *const u8) -> u16;
    fn ns_get32(cp: *const u8) -> u32;
    fn ns_put16(value: u16, cp: *mut u8) -> c_int;
    fn ns_put32(value: u32, cp: *mut u8) -> c_int;
    fn ns_skiprr(msg: *const u8, eom: *const u8, section: c_int, count: c_int) -> c_int;
    static in6addr_any: In6Addr;
    static in6addr_loopback: In6Addr;
}

#[repr(C)]
struct In6Addr {
    s6_addr: [u8; 16],
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: exit_group never returns.
    unsafe { exit(127) }
}
