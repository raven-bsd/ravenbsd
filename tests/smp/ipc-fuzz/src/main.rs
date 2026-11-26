// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K469 scheme-IPC message fuzz.
//!
//! One process registers a scheme ("fuzz"), so it is both the server and the client. The main
//! thread serves requests (`RECV`/`SEND`); a client thread `OPEN`s the scheme and issues one `CALL`
//! per group. The server replies with deterministic malformed `SchemePacket`s: bad header/payload
//! lengths, unknown opcodes, wrong opcode, oversized result bounds, attachment indices and rights
//! masks. Every send records its return; the client records the request result, so the peer's view
//! of each malformed reply is measured too. It fixes nothing; deviations are findings.
//!
//! The 536-byte packet is the kernel's `SchemePacket`; the fixture uses the same `raven-ipc`
//! codec, so the wire image cannot drift from the kernel's.

#![no_std]
#![no_main]

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_ipc::protocol::{Opcode, PACKET_SIZE, SchemePacket, decode, encode};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

/// Deterministic generator seed (the vectors are fixed; the seed labels the run).
const SEED: u64 = 0x4950_4346_555A_5A00;
/// Sentinel request payload that ends the server loop.
const SENTINEL: [u8; 8] = *b"IPCFUZZ!";
/// Client `CALL` response capacity.
const RESPONSE_CAP: usize = 64;
/// Raw `pread(handle, buf, len, offset)` number (PG-22, kernel-owned).
const PREAD: usize = 0x6b;
/// Raw `seek(handle, offset, whence)` number (PG-22, kernel-owned).
const SEEK: usize = 0x6d;
/// Attachments the harness owns for the rights vectors.
const MAX_GROUPS: usize = 20;

const EBADF: &[Errno] = &[Errno::EBADF];
const EINVAL: &[Errno] = &[Errno::EINVAL];
const EPROTO: &[Errno] = &[Errno::EPROTO];
/// The request handle is retired (closed) when a reply completes the request, so a second SEND
/// is EBADF; EIO/EPROTO are accepted for a race with the retire.
const FOLLOW_AFTER_COMPLETE: &[Errno] = &[Errno::EBADF, Errno::EIO, Errno::EPROTO];
/// A huge client length fails in the capture allocator with ENOMEM before the range check.
const HUGE_LENGTH: &[Errno] = &[Errno::EFAULT, Errno::EINVAL, Errno::E2BIG];
const EBADF_EACCES: &[Errno] = &[Errno::EBADF, Errno::EACCES];
const WRAP_OFFSET: &[Errno] = &[Errno::EFAULT, Errno::EINVAL, Errno::EOVERFLOW];

#[derive(Clone, Copy)]
enum Expect {
    Success,
    Err(&'static [Errno]),
}

/// One malformed-reply group: one client `CALL`, zero or more server `SEND`s, then a valid reply.
#[derive(Clone, Copy)]
enum Group {
    Truncation,
    DataLen0,
    DataLen1,
    DataLenMax,
    Opcode0,
    Opcode99,
    WrongOpcode,
    WrongOpcodeLen,
    BufferOver,
    AttachCount5,
    AttachRange,
    AttachClosed,
    AttachDup,
    AttachRights,
    AttachMaskExtra,
}

const GROUPS: &[Group] = &[
    Group::Truncation,
    Group::DataLen0,
    Group::DataLen1,
    Group::DataLenMax,
    Group::Opcode0,
    Group::Opcode99,
    Group::WrongOpcode,
    Group::WrongOpcodeLen,
    Group::BufferOver,
    Group::AttachCount5,
    Group::AttachRange,
    Group::AttachClosed,
    Group::AttachDup,
    Group::AttachRights,
    Group::AttachMaskExtra,
];

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; 64 * 1024]);

static mut CLIENT_STACK: Stack = Stack([0; 64 * 1024]);
static mut CALL_REQ: [u8; 8] = [0; 8];
static mut CALL_RESP: [u8; RESPONSE_CAP] = [0; RESPONSE_CAP];

static CLIENT_DONE: AtomicBool = AtomicBool::new(false);
static CALL_RESULTS: [AtomicI64; MAX_GROUPS] = [const { AtomicI64::new(i64::MIN) }; MAX_GROUPS];
static CLIENT_ROWS: [AtomicI64; 10] = [const { AtomicI64::new(i64::MIN) }; 10];

/// The server-side attachment handles, published before the client thread starts.
static MEMOBJ: AtomicI64 = AtomicI64::new(-1);
static CLOSED: AtomicI64 = AtomicI64::new(-1);

struct Table {
    rows: usize,
    deviations: usize,
    hash: u64,
}

impl Table {
    fn new() -> Self {
        Self {
            rows: 0,
            deviations: 0,
            hash: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn row(&mut self, direction: &str, group: &str, vector: &str, ret: isize, expect: Expect) {
        self.emit(direction, group, vector, ret, expect);
    }

    /// One row for a byte-boundary truncation; the numeric length is in the name and the hash.
    fn row_len(&mut self, group: &str, length: usize, ret: isize, expect: Expect) {
        self.emit_len(group, length, ret, expect);
    }

    fn emit(&mut self, direction: &str, group: &str, vector: &str, ret: isize, expect: Expect) {
        let ok = self.check(vector, ret, expect);
        println!(
            "ipc-fuzz: case dir={direction} group={group} vector={vector} ret={ret} errno={} ok={}",
            errno_label(ret, expect),
            if ok { "yes" } else { "no" }
        );
        self.finish(direction, group, vector.as_bytes(), ret);
    }

    fn emit_len(&mut self, group: &str, length: usize, ret: isize, expect: Expect) {
        let ok = self.check("len", ret, expect);
        println!(
            "ipc-fuzz: case dir=send group={group} vector=len{length} ret={ret} errno={} ok={}",
            errno_label(ret, expect),
            if ok { "yes" } else { "no" }
        );
        self.finish("send", group, b"len", ret);
        self.hash = fnv(self.hash, &(length as u64).to_le_bytes());
    }

    fn check(&mut self, vector: &str, ret: isize, expect: Expect) -> bool {
        let ok = match expect {
            Expect::Success => ret >= 0,
            Expect::Err(set) => {
                ret < 0 && set.contains(&Errno(u16::try_from(-ret).unwrap_or(u16::MAX)))
            }
        };
        if !ok {
            self.deviations = self.deviations.saturating_add(1);
            println!(
                "ipc-fuzz: deviation vector={vector} expected={} actual={ret}",
                describe(expect)
            );
        }
        ok
    }

    fn finish(&mut self, direction: &str, group: &str, vector: &[u8], ret: isize) {
        self.rows = self.rows.saturating_add(1);
        self.hash = fnv(self.hash, direction.as_bytes());
        self.hash = fnv(self.hash, group.as_bytes());
        self.hash = fnv(self.hash, vector);
        self.hash = fnv(self.hash, &(ret as i64).to_le_bytes());
    }

    fn summary(&self) {
        println!(
            "ipc-fuzz: summary rows={} deviations={} seed=0x{SEED:016x} table-hash=0x{:016x}",
            self.rows, self.deviations, self.hash
        );
    }
}

fn errno_label(ret: isize, _expect: Expect) -> &'static str {
    if ret < 0 {
        errno_name(Errno(u16::try_from(-ret).unwrap_or(u16::MAX)))
    } else {
        "none"
    }
}

fn describe(expect: Expect) -> &'static str {
    match expect {
        Expect::Success => "success",
        Expect::Err(_) => "documented-errno",
    }
}

fn errno_name(error: Errno) -> &'static str {
    match error {
        Errno::EPERM => "EPERM",
        Errno::EPIPE => "EPIPE",
        Errno::EACCES => "EACCES",
        Errno::EFAULT => "EFAULT",
        Errno::EBADF => "EBADF",
        Errno::EINVAL => "EINVAL",
        Errno::EIO => "EIO",
        Errno::E2BIG => "E2BIG",
        Errno::ENOMEM => "ENOMEM",
        Errno::ENAMETOOLONG => "ENAMETOOLONG",
        Errno::EPROTO => "EPROTO",
        Errno::EOVERFLOW => "EOVERFLOW",
        _ => "other",
    }
}

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn raw(number: usize, args: [usize; 6]) -> isize {
    // SAFETY: the number is a documented syscall and every pointer is a live static buffer or a
    // deliberate invalid value the kernel validates with `UserSlice`/fault-safe copy.
    unsafe { sys::syscall6(number, args[0], args[1], args[2], args[3], args[4], args[5]) }
}

fn ret(result: Result<(), Errno>) -> isize {
    match result {
        Ok(()) => 0,
        Err(error) => -(error.0 as isize),
    }
}

fn encode_packet(packet: &SchemePacket) -> Option<[u8; PACKET_SIZE]> {
    let mut bytes = [0u8; PACKET_SIZE];
    encode(packet, &mut bytes).ok()?;
    Some(bytes)
}

fn base_reply(request: &SchemePacket) -> SchemePacket {
    let opcode = Opcode::from_u16(request.opcode).unwrap_or(Opcode::Call);
    SchemePacket::request(opcode, request.request_id, request.file_id)
}

fn valid_reply(request: &SchemePacket) -> [u8; PACKET_SIZE] {
    let packet = base_reply(request);
    encode_packet(&packet).unwrap_or([0u8; PACKET_SIZE])
}

fn decode_request(bytes: &[u8; PACKET_SIZE]) -> Option<SchemePacket> {
    decode(bytes).ok()
}

/// Read the captured request payload (`buffer_addr`/`buffer_len` in the server's address space).
fn request_payload(packet: &SchemePacket) -> &'static [u8] {
    let length = usize::try_from(packet.buffer_len).unwrap_or(0);
    if length == 0 || length > 64 {
        return &[];
    }
    let pointer = packet.buffer_addr as usize as *const u8;
    // SAFETY: the kernel captured the client's request buffer at `buffer_addr` in this address
    // space with exactly `buffer_len` bytes; the slice is used only for the sentinel compare.
    unsafe { core::slice::from_raw_parts(pointer, length) }
}

fn main(_args: raven_rt::Args) -> i32 {
    println!(
        "ipc-fuzz: start seed=0x{SEED:016x} groups={} packet={PACKET_SIZE}",
        GROUPS.len()
    );
    let memobj = match sys::memobj_create_pager(4096) {
        Ok(handle) => handle,
        Err(error) => {
            println!("ipc-fuzz: setup-failed memobj errno={}", error.0);
            return 1;
        }
    };
    MEMOBJ.store(i64::from(memobj.0), Ordering::Release);
    let duplicate = match sys::handle_dup(memobj, Rights::ALL) {
        Ok(handle) => handle,
        Err(error) => {
            println!("ipc-fuzz: setup-failed dup errno={}", error.0);
            return 1;
        }
    };
    if let Err(error) = sys::handle_close(duplicate) {
        println!("ipc-fuzz: setup-failed close errno={}", error.0);
        return 1;
    }
    CLOSED.store(i64::from(duplicate.0), Ordering::Release);

    if let Err(error) = sys::scheme_register("fuzz") {
        println!("ipc-fuzz: setup-failed register errno={}", error.0);
        return 1;
    }
    let stack = core::ptr::addr_of_mut!(CLIENT_STACK);
    let top = (stack as usize).saturating_add(core::mem::size_of::<Stack>());
    if let Err(error) =
        sys::thread_create(Handle::SELF_PROC, client_main as *const () as usize, top, 0)
    {
        println!("ipc-fuzz: setup-failed thread errno={}", error.0);
        return 1;
    }

    let mut table = Table::new();
    let mut request_buf = [0u8; PACKET_SIZE];

    // The client's OPEN arrives first; a valid Open reply completes it.
    let open_handle = match sys::recv(&mut request_buf) {
        Ok(handle) => handle,
        Err(error) => {
            println!("ipc-fuzz: setup-failed open-recv errno={}", error.0);
            return 1;
        }
    };
    let open_request = match decode_request(&request_buf) {
        Some(packet) => packet,
        None => {
            println!("ipc-fuzz: setup-failed open-decode");
            return 1;
        }
    };
    let open_reply = valid_reply(&open_request);
    match sys::send(open_handle, &open_reply) {
        Ok(()) => {}
        Err(error) => {
            println!("ipc-fuzz: setup-failed open-reply errno={}", error.0);
            return 1;
        }
    }

    let memobj_handle = Handle(u32::try_from(MEMOBJ.load(Ordering::Acquire)).unwrap_or(0));
    let closed_handle = Handle(u32::try_from(CLOSED.load(Ordering::Acquire)).unwrap_or(0));

    for (index, group) in GROUPS.iter().copied().enumerate() {
        let request = match sys::recv(&mut request_buf) {
            Ok(handle) => handle,
            Err(error) => {
                println!("ipc-fuzz: group {index} recv-failed errno={}", error.0);
                return 1;
            }
        };
        let request_packet = match decode_request(&request_buf) {
            Some(packet) => packet,
            None => {
                println!("ipc-fuzz: group {index} decode-failed");
                return 1;
            }
        };
        let group_name = group_name(group);
        match group {
            Group::Truncation => {
                let packet = valid_reply(&request_packet);
                for length in 0..PACKET_SIZE {
                    let sent = ret(sys::send(request, &packet[..length]));
                    table.row_len(group_name, length, sent, Expect::Err(EINVAL));
                }
            }
            _ => {
                let (packet, expect, vector) =
                    malformed(group, &request_packet, memobj_handle, closed_handle);
                let bytes = encode_packet(&packet).unwrap_or([0u8; PACKET_SIZE]);
                let sent = ret(sys::send(request, &bytes));
                table.row("send", group_name, vector, sent, expect);
            }
        }
        let follow = valid_reply(&request_packet);
        let follow_ret = ret(sys::send(request, &follow));
        let follow_expect = match group {
            Group::BufferOver
            | Group::AttachRange
            | Group::AttachClosed
            | Group::DataLen0
            | Group::DataLen1
            | Group::AttachDup
            | Group::AttachRights
            | Group::AttachMaskExtra => Expect::Err(FOLLOW_AFTER_COMPLETE),
            _ => Expect::Success,
        };
        table.row(
            "send",
            group_name,
            "valid-followup",
            follow_ret,
            follow_expect,
        );
    }

    // The client's standalone raw requests (if any) and the sentinel arrive here. Every request is
    // answered so the client never hangs; the sentinel ends the loop.
    loop {
        let handle = match sys::recv(&mut request_buf) {
            Ok(handle) => handle,
            Err(_) => break,
        };
        let packet = match decode_request(&request_buf) {
            Some(packet) => packet,
            None => break,
        };
        let reply = valid_reply(&packet);
        let _ = sys::send(handle, &reply);
        if packet.opcode == Opcode::Call.as_u16() && request_payload(&packet) == &SENTINEL[..] {
            break;
        }
    }

    while !CLIENT_DONE.load(Ordering::Acquire) {
        let _ = sys::nanosleep(1_000_000);
    }

    for (index, group) in GROUPS.iter().copied().enumerate() {
        let value = CALL_RESULTS[index].load(Ordering::Acquire);
        let call_ret = if value == i64::MIN {
            -(Errno::EIO.0 as isize)
        } else {
            value as isize
        };
        let (expect, vector) = client_expectation(group);
        table.row("client", group_name(group), vector, call_ret, expect);
    }
    let standalone = [
        ("call-reqmax", Expect::Err(HUGE_LENGTH)),
        ("call-respmax", Expect::Err(HUGE_LENGTH)),
        ("read-lenmax", Expect::Err(HUGE_LENGTH)),
        ("pread-0", Expect::Success),
        ("pread-1", Expect::Success),
        ("pread-last", Expect::Success),
        ("pread-wrap", Expect::Err(WRAP_OFFSET)),
        ("seek-whence", Expect::Err(EINVAL)),
        ("send-file", Expect::Err(EBADF_EACCES)),
        ("send-forged", Expect::Err(EBADF)),
    ];
    for (index, (vector, expect)) in standalone.iter().enumerate() {
        let value = CLIENT_ROWS[index].load(Ordering::Acquire);
        let call_ret = if value == i64::MIN {
            -(Errno::EIO.0 as isize)
        } else {
            value as isize
        };
        table.row("client", "standalone", vector, call_ret, *expect);
    }

    table.summary();
    println!("ipc-fuzz: alive");
    0
}

fn group_name(group: Group) -> &'static str {
    match group {
        Group::Truncation => "truncation",
        Group::DataLen0 => "data-len-0",
        Group::DataLen1 => "data-len-1",
        Group::DataLenMax => "data-len-max",
        Group::Opcode0 => "opcode-0",
        Group::Opcode99 => "opcode-99",
        Group::WrongOpcode => "wrong-opcode",
        Group::WrongOpcodeLen => "wrong-opcode-len",
        Group::BufferOver => "buffer-over",
        Group::AttachCount5 => "attach-count-5",
        Group::AttachRange => "attach-range",
        Group::AttachClosed => "attach-closed",
        Group::AttachDup => "attach-dup",
        Group::AttachRights => "attach-rights",
        Group::AttachMaskExtra => "attach-mask-extra",
    }
}

fn malformed(
    group: Group,
    request: &SchemePacket,
    memobj: Handle,
    closed: Handle,
) -> (SchemePacket, Expect, &'static str) {
    let mut packet = base_reply(request);
    match group {
        Group::DataLen0 => {
            packet.data_len = 0;
            (packet, Expect::Success, "data-len-0")
        }
        Group::DataLen1 => {
            packet.data_len = 1;
            (packet, Expect::Success, "data-len-1")
        }
        Group::DataLenMax => {
            packet.data_len = u32::MAX;
            (packet, Expect::Err(EPROTO), "data-len-max")
        }
        Group::Opcode0 => {
            packet.opcode = 0;
            (packet, Expect::Err(EPROTO), "opcode-0")
        }
        Group::Opcode99 => {
            packet.opcode = 99;
            (packet, Expect::Err(EPROTO), "opcode-99")
        }
        Group::WrongOpcode => {
            packet.opcode = Opcode::Read.as_u16();
            (packet, Expect::Err(EPROTO), "opcode-read")
        }
        Group::WrongOpcodeLen => {
            packet.opcode = Opcode::Open.as_u16();
            packet.data_len = 257;
            (packet, Expect::Err(EPROTO), "open-len-257")
        }
        Group::BufferOver => {
            packet.buffer_len = RESPONSE_CAP as u64 + 1;
            (packet, Expect::Err(EPROTO), "buffer-65")
        }
        Group::AttachCount5 => {
            packet.attach_count = 5;
            (packet, Expect::Err(EPROTO), "attach-count-5")
        }
        Group::AttachRange => {
            packet.attach_count = 1;
            packet.attach[0].handle = 0x7fff_ffff;
            (packet, Expect::Err(EBADF), "handle-7fffffff")
        }
        Group::AttachClosed => {
            packet.attach_count = 1;
            packet.attach[0].handle = closed.0;
            (packet, Expect::Err(EBADF), "handle-closed")
        }
        Group::AttachDup => {
            packet.attach_count = 2;
            packet.attach[0].handle = memobj.0;
            packet.attach[1].handle = memobj.0;
            (packet, Expect::Success, "handle-dup")
        }
        Group::AttachRights => {
            packet.attach_count = 1;
            packet.attach[0].handle = memobj.0;
            packet.attach[0].rights_mask = u32::MAX;
            (packet, Expect::Success, "mask-all")
        }
        Group::AttachMaskExtra => {
            packet.attach_count = 1;
            packet.attach[0].handle = memobj.0;
            packet.attach[0].rights_mask = 0x8000_0000;
            (packet, Expect::Success, "mask-extra")
        }
        Group::Truncation => (packet, Expect::Err(EINVAL), "truncation"),
    }
}

fn client_expectation(group: Group) -> (Expect, &'static str) {
    match group {
        Group::BufferOver => (Expect::Err(EPROTO), "peer-EPROTO"),
        Group::AttachRange | Group::AttachClosed => (Expect::Err(EBADF), "peer-EBADF"),
        _ => (Expect::Success, "peer-ok"),
    }
}

/// Client thread: opens the scheme, one CALL per group, raw malformed calls, then the sentinel.
extern "C" fn client_main(_arg: usize) -> ! {
    let file = match sys::open("fuzz:", 1) {
        Ok(handle) => handle,
        Err(_) => {
            CLIENT_DONE.store(true, Ordering::Release);
            sys::thread_exit(1);
        }
    };
    let request = core::ptr::addr_of!(CALL_REQ) as usize;
    let response = core::ptr::addr_of_mut!(CALL_RESP) as usize;
    for (index, group) in GROUPS.iter().copied().enumerate() {
        let result = sys::call(file, &[0u8; 8], unsafe {
            // SAFETY: the response buffer is a live static of exactly RESPONSE_CAP bytes.
            core::slice::from_raw_parts_mut(response as *mut u8, RESPONSE_CAP)
        });
        let value = match result {
            Ok(length) => length as i64,
            Err(error) => -i64::from(error.0),
        };
        CALL_RESULTS[index].store(value, Ordering::Release);
        let _ = group;
    }

    let raw_rows = [
        raw(
            raven_abi::syscall::Syscall::CALL as usize,
            [
                file.0 as usize,
                request,
                usize::MAX,
                response,
                RESPONSE_CAP,
                0,
            ],
        ),
        raw(
            raven_abi::syscall::Syscall::CALL as usize,
            [file.0 as usize, request, 8, response, usize::MAX, 0],
        ),
        raw(
            raven_abi::syscall::Syscall::READ as usize,
            [file.0 as usize, response, usize::MAX, 0, 0, 0],
        ),
        raw(PREAD, [file.0 as usize, response, 16, 0, 0, 0]),
        raw(PREAD, [file.0 as usize, response, 16, 1, 0, 0]),
        raw(
            PREAD,
            [file.0 as usize, response, 16, usize::MAX - 16, 0, 0],
        ),
        raw(PREAD, [file.0 as usize, response, 16, usize::MAX - 7, 0, 0]),
        raw(SEEK, [file.0 as usize, 0, usize::MAX, 0, 0, 0]),
        ret(sys::send(file, &valid_placeholder())),
        ret(sys::send(Handle(0x7fff_ffff), &valid_placeholder())),
    ];
    for (index, value) in raw_rows.iter().enumerate() {
        CLIENT_ROWS[index].store(*value as i64, Ordering::Release);
    }

    let mut sentinel_resp = [0u8; RESPONSE_CAP];
    let _ = sys::call(file, &SENTINEL, &mut sentinel_resp);
    let _ = sys::close(file);
    CLIENT_DONE.store(true, Ordering::Release);
    sys::thread_exit(0);
}

/// A syntactically valid packet used only to attempt `SEND` on non-request handles.
fn valid_placeholder() -> [u8; PACKET_SIZE] {
    let packet = SchemePacket::request(Opcode::Call, 0, 0);
    encode_packet(&packet).unwrap_or([0u8; PACKET_SIZE])
}
