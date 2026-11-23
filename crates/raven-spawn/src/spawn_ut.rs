// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for stack layout, initfs spawning and file-image spawning.

use super::*;
use raven_abi::layout::x86_64::USER_END;
use std::cell::RefCell;

const TEST_TOP: u64 = 0x8000_0000;
const TEST_BUFFER: usize = 64 * 1024;
const TEST_ID: ProcessIdentity = ProcessIdentity {
    uid: 1000,
    euid: 0,
    gid: 1000,
    egid: 0,
};

fn build(
    path: &str,
    args: &[&str],
    env: &[&str],
    auxv: &[(usize, usize)],
) -> (Vec<u8>, usize, u64) {
    let mut buffer = vec![0u8; TEST_BUFFER];
    let (offset, sp0) = build_stack(
        &mut buffer,
        TEST_TOP,
        path,
        args,
        env,
        b"/",
        auxv,
        &[0xAB; RANDOM_LEN],
        "x86_64",
    )
    .expect("stack image should fit");
    (buffer, offset, sp0)
}

fn base() -> u64 {
    TEST_TOP - TEST_BUFFER as u64
}

fn word(buffer: &[u8], address: u64) -> usize {
    let offset = usize::try_from(address - base()).unwrap();
    usize::from_le_bytes(buffer[offset..offset + WORD].try_into().unwrap())
}

fn bytes_at(buffer: &[u8], address: usize) -> Vec<u8> {
    let address = address as u64;
    let offset = usize::try_from(address - base()).unwrap();
    let end = buffer[offset..]
        .iter()
        .position(|&byte| byte == 0)
        .map(|length| offset + length)
        .unwrap();
    buffer[offset..end].to_vec()
}

#[test]
fn empty_args_use_the_path_as_argv_zero() {
    let (buffer, _, sp0) = build("/child", &[], &[], &[]);
    assert_eq!(word(&buffer, sp0), 1);
    assert_eq!(
        bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)),
        b"/child"
    );
}

#[test]
fn one_argument_layout() {
    let (buffer, _, sp0) = build("/child", &["/child"], &[], &[]);
    assert_eq!(word(&buffer, sp0), 1);
    assert_eq!(
        bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)),
        b"/child"
    );
}

#[test]
fn three_argument_layout() {
    let (buffer, _, sp0) = build("/child", &["a", "b", "c"], &[], &[]);
    assert_eq!(word(&buffer, sp0), 3);
    for (index, expected) in ["a", "b", "c"].iter().enumerate() {
        let pointer = word(&buffer, sp0 + WORD as u64 * (index as u64 + 1));
        assert_eq!(bytes_at(&buffer, pointer), expected.as_bytes());
    }
    assert_eq!(word(&buffer, sp0 + WORD as u64 * 4), 0);
}

#[test]
fn empty_strings_are_preserved() {
    let (buffer, _, sp0) = build("/child", &["", "x", ""], &[], &[]);
    assert_eq!(word(&buffer, sp0), 3);
    assert!(bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)).is_empty());
    assert_eq!(
        bytes_at(&buffer, word(&buffer, sp0 + WORD as u64 * 3)),
        b"".to_vec()
    );
}

#[test]
fn env_pointers_follow_argv_null() {
    let (buffer, _, sp0) = build("/child", &["/child"], &["A=1", "B=2"], &[]);
    let argc = word(&buffer, sp0) as u64;
    let argv_null = sp0 + WORD as u64 * (1 + argc);
    assert_eq!(word(&buffer, argv_null), 0);
    let env0 = argv_null + WORD as u64;
    let env1 = env0 + WORD as u64;
    assert_eq!(bytes_at(&buffer, word(&buffer, env0)), b"A=1");
    assert_eq!(bytes_at(&buffer, word(&buffer, env1)), b"B=2");
    assert_eq!(word(&buffer, env1 + WORD as u64), 0);
}

#[test]
fn auxv_pairs_and_terminator() {
    let (buffer, _, sp0) = build("/child", &["/child"], &[], &[(6, 4096), (9, 0x400000)]);
    let argc = word(&buffer, sp0) as u64;
    let argv_null = sp0 + WORD as u64 * (1 + argc);
    let env0 = argv_null + WORD as u64;
    let env_null = env0;
    assert_eq!(word(&buffer, env_null), 0);
    let auxv0 = env_null + WORD as u64;
    assert_eq!(word(&buffer, auxv0), 6);
    assert_eq!(word(&buffer, auxv0 + WORD as u64), 4096);
    assert_eq!(word(&buffer, auxv0 + 2 * WORD as u64), 9);
    assert_eq!(word(&buffer, auxv0 + 3 * WORD as u64), 0x400000);
    assert_eq!(word(&buffer, auxv0 + 4 * WORD as u64), 0);
    assert_eq!(word(&buffer, auxv0 + 5 * WORD as u64), 0);
}

#[test]
fn at_random_points_at_sixteen_bytes() {
    let (buffer, _, sp0) = build("/child", &["/child"], &[], &[(25, 0)]);
    let argc = word(&buffer, sp0) as u64;
    let argv_null = sp0 + WORD as u64 * (1 + argc);
    let auxv0 = argv_null + 2 * WORD as u64;
    let pointer = word(&buffer, auxv0 + WORD as u64);
    let offset = usize::try_from(pointer as u64 - base()).unwrap();
    assert_eq!(buffer[offset..offset + RANDOM_LEN], [0xAB; RANDOM_LEN]);
}

#[test]
fn sp0_is_sixteen_byte_aligned() {
    let (_, _, sp0) = build("/child", &["a", "b"], &["A=1"], &[]);
    assert_eq!(sp0 % 16, 0);
}

#[test]
fn all_pointers_are_inside_the_stack_mapping() {
    let (buffer, _, sp0) = build("/child", &["a", "bc"], &["A=1"], &[(6, 4096)]);
    let argc = word(&buffer, sp0) as u64;
    for index in 0..argc {
        let pointer = word(&buffer, sp0 + WORD as u64 * (index + 1));
        assert!(pointer as u64 >= base() && (pointer as u64) < TEST_TOP);
    }
    let env0 = sp0 + WORD as u64 * (1 + argc + 1);
    let pointer = word(&buffer, env0);
    assert!(pointer as u64 >= base() && (pointer as u64) < TEST_TOP);
}

#[test]
fn strings_are_in_argument_order() {
    let (buffer, _, sp0) = build("/child", &["first", "second", "third"], &[], &[]);
    let first = word(&buffer, sp0 + WORD as u64);
    let second = word(&buffer, sp0 + 2 * WORD as u64);
    let third = word(&buffer, sp0 + 3 * WORD as u64);
    assert!(first < second && second < third);
    assert_eq!(bytes_at(&buffer, first), b"first");
    assert_eq!(bytes_at(&buffer, second), b"second");
    assert_eq!(bytes_at(&buffer, third), b"third");
}

#[test]
fn environment_strings_follow_argument_strings() {
    let (buffer, _, sp0) = build("/child", &["arg"], &["ENV=1"], &[]);
    let arg = word(&buffer, sp0 + WORD as u64);
    let env0 = sp0 + WORD as u64 * 3;
    let env = word(&buffer, env0);
    assert!(arg < env);
    assert_eq!(bytes_at(&buffer, env), b"ENV=1");
}

#[test]
fn overlong_strings_are_too_large() {
    let long = "x".repeat(TEST_BUFFER + 1);
    let mut buffer = vec![0u8; TEST_BUFFER];
    assert!(matches!(
        build_stack(
            &mut buffer,
            TEST_TOP,
            "/child",
            &[&long],
            &[],
            b"/",
            &[],
            &[0; 16],
            "x86_64"
        ),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn too_many_arguments_are_too_large() {
    let names: Vec<&str> = vec![""; 9000];
    let mut buffer = vec![0u8; TEST_BUFFER];
    assert!(matches!(
        build_stack(
            &mut buffer,
            TEST_TOP,
            "/child",
            &names,
            &[],
            b"/",
            &[],
            &[0; 16],
            "x86_64"
        ),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn misaligned_stack_top_is_rejected() {
    let mut buffer = vec![0u8; TEST_BUFFER];
    assert!(matches!(
        build_stack(
            &mut buffer,
            TEST_TOP + 1,
            "/child",
            &[],
            &[],
            b"/",
            &[],
            &[0; 16],
            "x86_64"
        ),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn small_buffer_is_rejected() {
    let mut buffer = [0u8; 64];
    assert!(matches!(
        build_stack(
            &mut buffer,
            TEST_TOP,
            "/child",
            &["a"],
            &[],
            b"/",
            &[],
            &[0; 16],
            "x86_64"
        ),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn image_only_touches_the_top_of_the_buffer() {
    let mut buffer = vec![0xFFu8; TEST_BUFFER];
    let (offset, _) = build_stack(
        &mut buffer,
        TEST_TOP,
        "/child",
        &["a"],
        &[],
        b"/",
        &[],
        &[0; 16],
        "x86_64",
    )
    .unwrap();
    assert_eq!(&buffer[..offset], vec![0xFFu8; offset]);
    assert!(buffer[offset..].iter().any(|&byte| byte != 0xFF));
}

#[test]
fn no_environment_has_one_null_word() {
    let (buffer, _, sp0) = build("/child", &["/child"], &[], &[]);
    let argc = word(&buffer, sp0) as u64;
    let env0 = sp0 + WORD as u64 * (1 + argc + 1);
    assert_eq!(word(&buffer, env0), 0);
}

#[test]
fn auxv_without_random_keeps_the_value() {
    let (buffer, _, sp0) = build("/child", &["/child"], &[], &[(6, 4096)]);
    let argc = word(&buffer, sp0) as u64;
    let argv_null = sp0 + WORD as u64 * (1 + argc);
    let auxv0 = argv_null + 2 * WORD as u64;
    assert_eq!(word(&buffer, auxv0), 6);
    assert_eq!(word(&buffer, auxv0 + WORD as u64), 4096);
}

#[test]
fn same_inputs_produce_the_same_image() {
    let first = build("/child", &["a", "b"], &["A=1"], &[(6, 4096)]);
    let second = build("/child", &["a", "b"], &["A=1"], &[(6, 4096)]);
    assert_eq!(first.1, second.1);
    assert_eq!(first.2, second.2);
    assert_eq!(first.0, second.0);
}

const CHILD: Handle = Handle(0x10);
const MEMOBJ: Handle = Handle(0x20);
const THREAD: Handle = Handle(0x30);
const FILE: Handle = Handle(0x40);
const SCRATCH: usize = 0x7000_0000;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    ProcCreate,
    MemobjCreate(usize),
    Map {
        proc: u32,
        memobj: u32,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    },
    Unmap {
        proc: u32,
        addr: usize,
        len: usize,
    },
    Protect {
        proc: u32,
        addr: usize,
        len: usize,
        protection: u8,
    },
    ThreadCreate {
        proc: u32,
        entry: usize,
        stack_top: usize,
        arg: usize,
    },
    ExecCred {
        proc: u32,
        file: u32,
        nosuid: bool,
    },
    HandleClose(u32),
    WriteParentMem {
        addr: usize,
        len: usize,
    },
    CopyMapping {
        destination: usize,
        source: usize,
        len: usize,
    },
}

#[derive(Default)]
struct Mock {
    calls: RefCell<Vec<Call>>,
    fail_proc: bool,
    fail_memobj: bool,
    fail_map: Option<usize>,
    fail_unmap: bool,
    fail_close: Option<u32>,
    fail_thread: bool,
    fail_exec: bool,
    maps: usize,
    image: RefCell<Vec<u8>>,
}

impl Mock {
    fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    fn push(&self, call: Call) {
        self.calls.borrow_mut().push(call);
    }

    fn map_failure(&mut self) -> bool {
        self.maps += 1;
        self.fail_map == Some(self.maps)
    }
}

impl Sys for Mock {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        self.push(Call::ProcCreate);
        if self.fail_proc {
            Err(Errno::EIO)
        } else {
            Ok(CHILD)
        }
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
        self.push(Call::MemobjCreate(len));
        if self.fail_memobj {
            Err(Errno::EIO)
        } else {
            Ok(MEMOBJ)
        }
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno> {
        self.push(Call::Map {
            proc: proc.0,
            memobj: memobj.0,
            offset,
            addr,
            len,
            flags,
        });
        if self.map_failure() {
            return Err(Errno::EIO);
        }
        if proc == Handle::SELF_PROC && addr == 0 {
            Ok(SCRATCH)
        } else {
            Ok(addr)
        }
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        self.push(Call::Unmap {
            proc: proc.0,
            addr,
            len,
        });
        if self.fail_unmap {
            Err(Errno::EIO)
        } else {
            Ok(())
        }
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        self.push(Call::Protect {
            proc: proc.0,
            addr,
            len,
            protection,
        });
        Ok(())
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        self.push(Call::ThreadCreate {
            proc: proc.0,
            entry,
            stack_top,
            arg,
        });
        if self.fail_thread {
            Err(Errno::EIO)
        } else {
            Ok(THREAD)
        }
    }

    fn exec_cred(&mut self, proc: Handle, file: Handle, nosuid: bool) -> Result<(), Errno> {
        self.push(Call::ExecCred {
            proc: proc.0,
            file: file.0,
            nosuid,
        });
        if self.fail_exec {
            Err(Errno::EIO)
        } else {
            Ok(())
        }
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        self.push(Call::HandleClose(handle.0));
        if self.fail_close == Some(handle.0) {
            Err(Errno::EIO)
        } else {
            Ok(())
        }
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.push(Call::WriteParentMem {
            addr,
            len: bytes.len(),
        });
        *self.image.borrow_mut() = bytes.to_vec();
    }

    fn copy_mapping(&mut self, destination: usize, source: usize, len: usize) {
        self.push(Call::CopyMapping {
            destination,
            source,
            len,
        });
    }
}

fn child_elf(interp: bool, partial_page: bool) -> Vec<u8> {
    const PAGE: usize = 4096;
    const SEGMENT: u64 = 0x0040_0000;
    let mut elf = vec![0u8; PAGE];
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2;
    elf[5] = 1;
    elf[6] = 1;
    put_u16(&mut elf, 16, 2);
    put_u16(&mut elf, 18, 62);
    put_u32(&mut elf, 20, 1);
    put_u64(&mut elf, 24, SEGMENT + 0x100);
    put_u64(&mut elf, 32, 64);
    put_u16(&mut elf, 52, 64);
    put_u16(&mut elf, 54, 56);
    put_u16(&mut elf, 56, if interp { 2 } else { 1 });

    put_u32(&mut elf, 64, 1);
    put_u32(&mut elf, 68, 5);
    put_u64(&mut elf, 72, 0);
    put_u64(&mut elf, 80, SEGMENT);
    put_u64(&mut elf, 88, 0);
    put_u64(&mut elf, 96, if partial_page { 0x800 } else { 0x1000 });
    put_u64(&mut elf, 104, 0x1000);
    put_u64(&mut elf, 112, 0x1000);

    if interp {
        put_u32(&mut elf, 120, 3);
        put_u32(&mut elf, 124, 4);
        put_u64(&mut elf, 128, 0x200);
        put_u64(&mut elf, 136, 0);
        put_u64(&mut elf, 144, 0);
        put_u64(&mut elf, 152, 11);
        put_u64(&mut elf, 160, 11);
        put_u64(&mut elf, 168, 1);
        elf[0x200..0x20b].copy_from_slice(b"/lib/ld.so\0");
    }

    elf[0x100..0x10b].copy_from_slice(&[
        0xb8, 0x01, 0x00, 0x00, 0x00, 0x31, 0xff, 0x0f, 0x05, 0x0f, 0x0b,
    ]);
    elf
}

fn initfs_with(elf: &[u8]) -> Vec<u8> {
    const IMAGE: usize = 8192;
    let mut image = vec![0u8; IMAGE];
    image[0..8].copy_from_slice(b"RAVENFS1");
    put_u32(&mut image, 8, 1);
    put_u32(&mut image, 12, 64);
    put_u32(&mut image, 16, 1);
    put_u32(&mut image, 20, 48);
    put_u64(&mut image, 24, 112);
    put_u64(&mut image, 32, 6);
    put_u64(&mut image, 40, IMAGE as u64);
    put_u32(&mut image, 64, 0);
    put_u32(&mut image, 68, 6);
    put_u64(&mut image, 72, 4096);
    put_u64(&mut image, 80, 4096);
    put_u32(&mut image, 88, 0o755);
    put_u32(&mut image, 92, raven_initfs::crc32(elf));
    image[112..118].copy_from_slice(b"/child");
    image[4096..8192].copy_from_slice(elf);
    image
}

fn valid_initfs() -> Vec<u8> {
    initfs_with(&child_elf(false, false))
}

fn put_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn options<'a>(args: &'a [&'a str], env: &'a [&'a str]) -> SpawnOptions<'a> {
    SpawnOptions { args, env }
}

fn run(mock: &mut Mock) -> Result<Handle, SpawnError> {
    let image = valid_initfs();
    let initfs = Initfs::parse(&image).unwrap();
    spawn_with(&initfs, "/child", &options(&["/child", "world"], &[]), mock)
}

#[test]
fn success_call_sequence() {
    let image = valid_initfs();
    let initfs = Initfs::parse(&image).unwrap();
    let data_offset = initfs.get("/child").unwrap().data_offset as usize;
    let mut mock = Mock::default();
    let child = spawn_with(
        &initfs,
        "/child",
        &options(&["/child", "world"], &["A=B"]),
        &mut mock,
    )
    .unwrap();
    assert_eq!(child, CHILD);
    let calls = mock.calls();
    assert_eq!(calls.len(), 10, "{calls:?}");
    assert_eq!(calls[0], Call::ProcCreate);
    assert_eq!(
        calls[1],
        Call::Map {
            proc: CHILD.0,
            memobj: Handle::INITFS.0,
            offset: data_offset,
            addr: 0x0040_0000,
            len: 0x1000,
            flags: prot::READ | prot::EXEC,
        }
    );
    assert_eq!(calls[2], Call::MemobjCreate(STACK_SIZE));
    assert_eq!(
        calls[3],
        Call::Map {
            proc: Handle::SELF_PROC.0,
            memobj: MEMOBJ.0,
            offset: 0,
            addr: 0,
            len: STACK_SIZE,
            flags: map::pack(prot::READ | prot::WRITE, map::SHARED),
        }
    );
    assert!(matches!(&calls[4], Call::WriteParentMem { len, .. } if *len > 0));
    let (stack_base, flags) = match &calls[5] {
        Call::Map { addr, flags, .. } => (*addr, *flags),
        other => panic!("expected child stack map, got {other:?}"),
    };
    assert_eq!(stack_base, usize::try_from(USER_END).unwrap() - STACK_SIZE);
    // K196: the child stack is private so PROC_FORK can copy-on-write it.
    assert_eq!(flags, map::pack(prot::READ | prot::WRITE, map::FIXED));
    assert_eq!(
        calls[6],
        Call::Unmap {
            proc: Handle::SELF_PROC.0,
            addr: SCRATCH,
            len: STACK_SIZE,
        }
    );
    assert_eq!(calls[7], Call::HandleClose(MEMOBJ.0));
    // C8: the stack pointer is the `stack_top` kernel register (rsp/sp); the argument
    // register is not the C8 stack ABI, so it is deliberately not tied to `stack_top`.
    assert!(matches!(
        &calls[8],
        Call::ThreadCreate {
            proc,
            entry: 0x0040_0100,
            stack_top,
            ..
        } if *proc == CHILD.0 && stack_top % 16 == 0 && *stack_top >= stack_base
    ));
    assert_eq!(calls[9], Call::HandleClose(THREAD.0));
}

#[test]
fn an_image_open_requests_the_read_wire_bit() {
    let flags = image_open_flags().expect("image open flags");
    assert_eq!(
        flags.to_wire(),
        1,
        "raw O_RDONLY=0 is not a valid wire flag"
    );
}

#[test]
fn success_closes_the_thread_handle_and_keeps_the_child() {
    let image = valid_initfs();
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let child = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap();
    assert_eq!(child, CHILD);
    let calls = mock.calls();
    let closes: Vec<u32> = calls
        .iter()
        .filter_map(|call| match call {
            Call::HandleClose(handle) => Some(*handle),
            _ => None,
        })
        .collect();
    assert_eq!(closes.last(), Some(&THREAD.0));
    assert!(
        !closes.contains(&CHILD.0),
        "a successful spawn must keep the process handle: {closes:?}"
    );
}

#[test]
fn not_found_does_nothing() {
    let image = valid_initfs();
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let error = spawn_with(&initfs, "/missing", &options(&[], &[]), &mut mock).unwrap_err();
    assert_eq!(error, SpawnError::NotFound);
    assert!(mock.calls().is_empty());
}

#[test]
fn missing_interpreter_is_reported() {
    let image = initfs_with(&child_elf(true, false));
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let error = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap_err();
    assert_eq!(error, SpawnError::NotFound);
    assert!(mock.calls().is_empty());
}

fn interp_elf() -> Vec<u8> {
    let mut elf = child_elf(false, false);
    put_u16(&mut elf, 16, 3);
    elf
}

/// The release `ld-elf.so.1` shape: an `ET_DYN` interpreter whose first `PT_LOAD` is at
/// `vaddr=0`. The parser must accept the raw image; `plan_biased` applies `INTERP_BASE`.
fn interp_elf_at_zero() -> Vec<u8> {
    let mut elf = child_elf(false, false);
    put_u16(&mut elf, 16, 3);
    put_u64(&mut elf, 24, 0x100);
    put_u64(&mut elf, 80, 0);
    elf
}

#[test]
fn interpreter_at_zero_parses_before_biasing() {
    let interpreter = interp_elf_at_zero();
    let raw = Elf::parse(&interpreter, target_machine(), USER_MIN..USER_END);
    match raw {
        Err(error) => assert_eq!(
            error,
            ElfError::SegmentOutsideUserRange,
            "the old parse window must reject the raw vaddr-0 interpreter"
        ),
        Ok(_) => panic!("the old parse window unexpectedly accepted vaddr=0"),
    }

    let (_, plan, bias) = plan_image(&interpreter, true).expect("vaddr-0 interpreter must plan");
    assert_eq!(bias, INTERP_BASE);
    let mut steps = plan;
    let first = steps
        .next()
        .expect("the interpreter has at least one load step");
    assert_eq!(
        first.vaddr, INTERP_BASE,
        "the first step is the biased base, not page 0"
    );
    assert!(first.vaddr >= USER_MIN);
    for step in steps {
        assert!(step.vaddr >= USER_MIN);
        assert!(step.vaddr.saturating_add(step.len) <= USER_END);
    }
}

fn two_file_initfs(main: &[u8], interpreter: &[u8]) -> Vec<u8> {
    let mut builder = raven_initfs::InitfsBuilder::new();
    builder.add_file("/child", main, 0o755).unwrap();
    builder.add_file("/lib/ld.so", interpreter, 0o755).unwrap();
    builder.build()
}

#[test]
fn interpreter_is_mapped_and_the_thread_starts_at_its_entry() {
    let main = child_elf(true, false);
    let interpreter = interp_elf();
    let image = two_file_initfs(&main, &interpreter);
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let child = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap();
    assert_eq!(child, CHILD);
    let calls = mock.calls();
    let interp_base = usize::try_from(INTERP_BASE).unwrap();
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::Map {
            proc,
            memobj,
            addr,
            ..
        } if *proc == CHILD.0
            && *memobj == Handle::INITFS.0
            && *addr == interp_base + 0x0040_0000
    )));
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::Map {
            proc,
            memobj,
            addr,
            ..
        } if *proc == CHILD.0
            && *memobj == Handle::INITFS.0
            && *addr == 0x0040_0000
    )));
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::ThreadCreate { entry, .. } if *entry == interp_base + 0x0040_0100
    )));
}

#[test]
fn initfs_copy_tail_maps_the_source_private() {
    let mut main = child_elf(true, true);
    put_u16(&mut main, 16, 3);
    let interpreter = interp_elf();
    let image = two_file_initfs(&main, &interpreter);
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap();
    let calls = mock.calls();
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::CopyMapping { len, .. } if *len == 0x800
    )));
    for call in &calls {
        if let Call::Map {
            proc,
            memobj,
            flags,
            ..
        } = call
            && *proc == Handle::SELF_PROC.0
            && *memobj == Handle::INITFS.0
        {
            assert_eq!(
                *flags & map::pack(0, map::SHARED),
                0,
                "initfs source map must be private: {call:?}"
            );
        }
    }
}

#[test]
fn platform_and_execfn_point_into_the_stack() {
    let (buffer, _, sp0) = build(
        "/child",
        &["/child"],
        &[],
        &[(AT_PLATFORM, 0), (AT_EXECFN, 0)],
    );
    let argc = word(&buffer, sp0) as u64;
    let argv_null = sp0 + WORD as u64 * (1 + argc);
    let auxv0 = argv_null + 2 * WORD as u64;
    assert_eq!(word(&buffer, auxv0), AT_PLATFORM);
    let platform = word(&buffer, auxv0 + WORD as u64);
    assert_eq!(bytes_at(&buffer, platform), b"x86_64");
    assert_eq!(word(&buffer, auxv0 + 2 * WORD as u64), AT_EXECFN);
    let execfn = word(&buffer, auxv0 + 3 * WORD as u64);
    assert_eq!(bytes_at(&buffer, execfn), b"/child");
}

#[test]
fn a_dynamic_partial_page_uses_copy_tail() {
    let mut main = child_elf(true, true);
    put_u16(&mut main, 16, 3);
    let interpreter = interp_elf();
    let mut mock = Mock::default();
    spawn_image_with_interp(
        &main,
        Some(&interpreter),
        "/child",
        &options(&[], &[]),
        &mut mock,
    )
    .unwrap();
    let calls = mock.calls();
    assert!(calls.contains(&Call::MemobjCreate(4096)));
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::CopyMapping { len, .. } if *len == 0x800
    )));
    assert!(calls.iter().any(|call| matches!(
        call,
        Call::Protect {
            proc,
            addr,
            protection,
            ..
        } if *proc == CHILD.0 && *addr == usize::try_from(MAIN_PIE_BASE).unwrap() + 0x0040_0000 && usize::from(*protection) == prot::READ | prot::EXEC
    )));
}

#[test]
fn auxv_has_the_pg50_keys_and_at_base() {
    let interpreter = interp_elf();
    let main = child_elf(true, false);
    let main_elf = Elf::parse(&main, target_machine(), USER_MIN..USER_END).unwrap();
    let (auxv, len) = build_auxv(&main_elf, 0, INTERP_BASE, None, None, TEST_ID, true);
    let pairs = &auxv[..len];
    let has = |key: usize, value: Option<usize>| {
        pairs
            .iter()
            .any(|(k, v)| *k == key && value.is_none_or(|want| *v == want))
    };
    assert!(has(AT_BASE, Some(usize::try_from(INTERP_BASE).unwrap())));
    assert!(has(AT_PHENT, Some(PHENT)));
    assert!(has(AT_PHNUM, None));
    assert!(has(AT_ENTRY, Some(0x0040_0100)));
    assert!(has(AT_PLATFORM, Some(0)));
    assert!(has(AT_EXECFN, Some(0)));
    assert!(has(AT_MINSIGSTKSZ, Some(8192)));
    assert!(has(AT_UID, Some(1000)));
    assert!(has(AT_EUID, Some(0)));
    assert!(has(AT_GID, Some(1000)));
    assert!(has(AT_EGID, Some(0)));
    assert!(has(AT_SECURE, Some(1)));
    let interp_elf = Elf::parse(&interpreter, target_machine(), USER_MIN..USER_END).unwrap();
    assert_eq!(main_elf.interp(), Some("/lib/ld.so"));
    assert_eq!(interp_elf.interp(), None);
}

#[test]
fn no_alloc_fdmap_rejects_inherited_descriptors() {
    assert_eq!(no_alloc_fdmap(&[]), Ok(()));
    assert_eq!(
        no_alloc_fdmap(&[(1, 7)]),
        Err(SpawnError::Sys(Errno::ENOTSUP))
    );
}

#[test]
fn fdmap_auxv_and_the_rfdm_block_are_encoded() {
    let main = child_elf(false, false);
    let elf = Elf::parse(&main, target_machine(), USER_MIN..USER_END).unwrap();
    let (auxv, len) = build_auxv(&elf, 0, 0, None, Some((0x7fff_0000, 2)), TEST_ID, false);
    let pairs = &auxv[..len];
    let has = |key: usize, value: usize| pairs.iter().any(|(k, v)| *k == key && *v == value);
    assert!(has(0x7003, 0x7fff_0000));
    assert!(has(0x7004, 2));
    assert!(has(0x7006, raven_abi::ABI_VERSION as usize));

    let block = encode_fdmap(&[(1, 7), (2, 8)]).unwrap();
    assert_eq!(block.get(..4), Some(b"RFDM".as_slice()));
    assert_eq!(block.get(6..8), Some(2u16.to_le_bytes().as_slice()));
    assert_eq!(block.len(), 8 + 2 * 18);
}

#[test]
fn strict_tail_is_rejected() {
    let image = initfs_with(&child_elf(false, true));
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let error = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap_err();
    assert_eq!(error, SpawnError::BadElf(ElfError::FileTailNotPageAligned));
    assert!(mock.calls().is_empty());
}

#[test]
fn proc_create_failure_has_nothing_to_clean() {
    let mut mock = Mock {
        fail_proc: true,
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    assert_eq!(mock.calls(), vec![Call::ProcCreate]);
}

#[test]
fn segment_map_failure_closes_the_child() {
    let mut mock = Mock {
        fail_map: Some(1),
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
    assert!(!mock.calls().contains(&Call::MemobjCreate(STACK_SIZE)));
}

#[test]
fn memobj_failure_closes_the_child() {
    let mut mock = Mock {
        fail_memobj: true,
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
}

#[test]
fn scratch_map_failure_closes_memobj_and_child() {
    let mut mock = Mock {
        fail_map: Some(2),
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    let calls = mock.calls();
    assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
    assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
}

#[test]
fn child_stack_map_failure_unmaps_and_closes() {
    let mut mock = Mock {
        fail_map: Some(3),
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    let calls = mock.calls();
    assert!(calls.contains(&Call::Unmap {
        proc: Handle::SELF_PROC.0,
        addr: SCRATCH,
        len: STACK_SIZE,
    }));
    assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
    assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
}

#[test]
fn unmap_failure_closes_memobj_and_child() {
    let mut mock = Mock {
        fail_unmap: true,
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    let calls = mock.calls();
    assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
    assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
}

#[test]
fn memobj_close_failure_closes_the_child() {
    let mut mock = Mock {
        fail_close: Some(MEMOBJ.0),
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
}

#[test]
fn thread_create_failure_closes_the_child() {
    let mut mock = Mock {
        fail_thread: true,
        ..Mock::default()
    };
    let error = run(&mut mock).unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
}

#[test]
fn oversized_stack_image_cleans_up() {
    let long = "x".repeat(STACK_SIZE + 1);
    let image = valid_initfs();
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let error = spawn_with(&initfs, "/child", &options(&[&long], &[]), &mut mock).unwrap_err();
    assert_eq!(error, SpawnError::TooLarge);
    let calls = mock.calls();
    assert!(calls.contains(&Call::Unmap {
        proc: Handle::SELF_PROC.0,
        addr: SCRATCH,
        len: STACK_SIZE,
    }));
    assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
    assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
    assert!(
        !calls
            .iter()
            .any(|call| matches!(call, Call::ThreadCreate { .. }))
    );
}

#[test]
fn parent_memory_contains_the_c8_stack() {
    let mut mock = Mock::default();
    run(&mut mock).unwrap();
    let image = mock.image.borrow();
    assert!(!image.is_empty());
    assert!(image.len() <= STACK_SIZE);
}

#[test]
fn image_spawn_copies_the_elf_and_protects_text() {
    let elf = child_elf(false, false);
    let mut mock = Mock::default();
    let child = spawn_image_with(
        &elf,
        "/child",
        &options(&["/child", "world"], &[]),
        &mut mock,
    )
    .unwrap();
    assert_eq!(child, CHILD);
    let calls = mock.calls();
    assert_eq!(calls[0], Call::MemobjCreate(4096));
    assert_eq!(
        calls[1],
        Call::Map {
            proc: Handle::SELF_PROC.0,
            memobj: MEMOBJ.0,
            offset: 0,
            addr: 0,
            len: 4096,
            flags: map::pack(prot::READ | prot::WRITE, map::SHARED),
        }
    );
    assert_eq!(
        calls[2],
        Call::WriteParentMem {
            addr: SCRATCH,
            len: 4096,
        }
    );
    assert_eq!(
        calls[3],
        Call::Unmap {
            proc: Handle::SELF_PROC.0,
            addr: SCRATCH,
            len: 4096,
        }
    );
    assert_eq!(calls[4], Call::ProcCreate);
    assert_eq!(
        calls[5],
        Call::Map {
            proc: CHILD.0,
            memobj: MEMOBJ.0,
            offset: 0,
            addr: 0x0040_0000,
            len: 0x1000,
            flags: prot::READ | prot::WRITE,
        }
    );
    assert_eq!(
        calls[6],
        Call::Protect {
            proc: CHILD.0,
            addr: 0x0040_0000,
            len: 0x1000,
            protection: (prot::READ | prot::EXEC) as u8,
        }
    );
    assert_eq!(calls[7], Call::HandleClose(MEMOBJ.0));
    assert!(matches!(&calls[8], Call::MemobjCreate(STACK_SIZE)));
}

#[test]
fn image_spawn_rejects_an_oversized_image() {
    let image = vec![0u8; MAX_FILE_IMAGE + 1];
    let mut mock = Mock::default();
    let error = spawn_image_with(&image, "/child", &options(&[], &[]), &mut mock).unwrap_err();
    assert_eq!(error, SpawnError::TooLarge);
    assert!(mock.calls().is_empty());
}

#[test]
fn exec_spawn_applies_credentials_before_the_first_thread() {
    let elf = child_elf(false, false);
    let mut mock = Mock::default();
    let child = spawn_image_with_exec(
        &elf,
        "/child",
        &options(&["/child"], &[]),
        FILE,
        false,
        &mut mock,
    )
    .unwrap();
    assert_eq!(child, CHILD);
    let calls = mock.calls();
    let exec = calls
        .iter()
        .position(|call| matches!(call, Call::ExecCred { .. }))
        .expect("exec credentials");
    assert_eq!(
        calls[exec],
        Call::ExecCred {
            proc: CHILD.0,
            file: FILE.0,
            nosuid: false,
        }
    );
    let stack_image = calls
        .iter()
        .enumerate()
        .skip(exec + 1)
        .find(|(_, call)| matches!(call, Call::WriteParentMem { .. }))
        .map(|(index, _)| index)
        .expect("stack image write after credentials");
    assert!(
        exec < stack_image,
        "credentials at {exec} must precede the auxv/stack image at {stack_image}: {calls:?}"
    );
    let thread = calls
        .iter()
        .position(|call| matches!(call, Call::ThreadCreate { .. }))
        .expect("thread create");
    assert!(exec < thread, "credentials must precede the first thread");
}

#[test]
fn exec_spawn_failure_closes_the_child_before_a_thread_starts() {
    let elf = child_elf(false, false);
    let mut mock = Mock {
        fail_exec: true,
        ..Mock::default()
    };
    let error = spawn_image_with_exec(
        &elf,
        "/child",
        &options(&["/child"], &[]),
        FILE,
        true,
        &mut mock,
    )
    .unwrap_err();
    assert_eq!(error, SpawnError::Sys(Errno::EIO));
    let calls = mock.calls();
    assert!(matches!(
        calls.last(),
        Some(Call::HandleClose(handle)) if *handle == CHILD.0
    ));
    assert!(
        !calls
            .iter()
            .any(|call| matches!(call, Call::ThreadCreate { .. }))
    );
}

#[test]
fn libc_auxv_carries_the_initfs_offset() {
    let main = child_elf(false, false);
    let elf = Elf::parse(&main, target_machine(), USER_MIN..USER_END).unwrap();
    let (auxv, len) = build_auxv(&elf, 0, 0, Some(0x4000), None, TEST_ID, false);
    let pairs = &auxv[..len];
    assert!(pairs.contains(&(AT_RAVEN_LIBC, 0x4000)));
}

#[test]
fn cwd_is_published_as_at_raven_cwd() {
    let mut buffer = vec![0u8; TEST_BUFFER];
    let (_, sp0) = build_stack(
        &mut buffer,
        TEST_TOP,
        "/child",
        &[],
        &[],
        b"/bin",
        &[(AT_RAVEN_CWD, 0)],
        &[0; 16],
        "x86_64",
    )
    .expect("stack image");
    let argc = word(&buffer, sp0);
    let word_size = WORD as u64;
    let auxv0 = sp0 + word_size * (argc as u64 + 3);
    let mut index = 0u64;
    let mut cwd = None;
    loop {
        let key = word(&buffer, auxv0 + index * 2 * word_size);
        if key == 0 {
            break;
        }
        if key == AT_RAVEN_CWD {
            cwd = Some(word(&buffer, auxv0 + (index * 2 + 1) * word_size));
        }
        index += 1;
    }
    let cwd = cwd.expect("AT_RAVEN_CWD is present");
    assert_eq!(bytes_at(&buffer, cwd), b"/bin");
}

#[test]
fn an_invalid_cwd_is_rejected_by_the_stack_builder() {
    let mut buffer = vec![0u8; TEST_BUFFER];
    for cwd in [b"".as_slice(), b"bin".as_slice(), b"/bad\0path".as_slice()] {
        assert!(matches!(
            build_stack(
                &mut buffer,
                TEST_TOP,
                "/child",
                &[],
                &[],
                cwd,
                &[],
                &[0; 16],
                "x86_64"
            ),
            Err(SpawnError::BadCwd)
        ));
    }
}

#[test]
fn a_megabyte_image_is_read_in_a_few_chunks() {
    let size = 984_000usize;
    let mut offset = 0usize;
    let mut calls = 0usize;
    let data = collect_exact(size, |buffer| {
        calls += 1;
        let length = (size - offset).min(buffer.len());
        buffer.get_mut(..length).expect("chunk").fill(0x5A);
        offset += length;
        Ok(length)
    })
    .expect("read");
    assert_eq!(data.len(), size);
    assert_eq!(calls, 1, "one known-size read must not probe EOF");
    assert!(data.iter().all(|byte| *byte == 0x5A));
}

#[test]
fn an_image_larger_than_the_bound_is_rejected() {
    let result = collect_exact(MAX_FILE_IMAGE + 1, |_buffer| Ok(0));
    assert!(matches!(result, Err(SpawnError::TooLarge)));
}

#[test]
fn a_short_read_is_an_io_error() {
    let mut calls = 0usize;
    let result = collect_exact(10, |buffer| {
        calls += 1;
        let length = if calls == 1 { 4 } else { 0 };
        buffer.get_mut(..length).expect("chunk").fill(1);
        Ok(length)
    });
    assert!(matches!(result, Err(SpawnError::Sys(Errno::EIO))));
    assert_eq!(calls, 2);
}

#[test]
fn c8_block_pins_argv_env_auxv_and_terminator() {
    let auxv = [
        (3, 0x40_0000),
        (4, 56),
        (5, 7),
        (6, 4096),
        (9, 0x40_1000),
        (25, 0),
    ];
    let (buffer, _, sp0) = build("/child", &["/child", "arg"], &["A=1", "B=2"], &auxv);
    assert_eq!(sp0 % 16, 0, "sp0 is 16-byte aligned");
    assert!(sp0 >= base() && sp0 < TEST_TOP);

    let argc = word(&buffer, sp0);
    assert_eq!(argc, 2);
    assert_eq!(
        bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)),
        b"/child"
    );
    assert_eq!(
        bytes_at(&buffer, word(&buffer, sp0 + 2 * WORD as u64)),
        b"arg"
    );
    let argv_null = sp0 + 3 * WORD as u64;
    assert_eq!(word(&buffer, argv_null), 0);
    let env0 = argv_null + WORD as u64;
    assert_eq!(bytes_at(&buffer, word(&buffer, env0)), b"A=1");
    let env1 = env0 + WORD as u64;
    assert_eq!(bytes_at(&buffer, word(&buffer, env1)), b"B=2");
    let env_null = env1 + WORD as u64;
    assert_eq!(word(&buffer, env_null), 0);

    let auxv0 = env_null + WORD as u64;
    for (index, (key, value)) in auxv.iter().enumerate() {
        let key_address = auxv0 + 2 * WORD as u64 * index as u64;
        assert_eq!(word(&buffer, key_address), *key, "auxv key {index}");
        let value_address = key_address + WORD as u64;
        if *key == 25 {
            let pointer = word(&buffer, value_address);
            let offset = usize::try_from(pointer as u64 - base()).expect("random offset");
            assert_eq!(buffer[offset..offset + RANDOM_LEN], [0xAB; RANDOM_LEN]);
        } else {
            assert_eq!(word(&buffer, value_address), *value, "auxv value {index}");
        }
    }
    let terminator = auxv0 + 2 * WORD as u64 * auxv.len() as u64;
    assert_eq!(word(&buffer, terminator), 0, "auxv terminator key");
    assert_eq!(
        word(&buffer, terminator + WORD as u64),
        0,
        "auxv terminator value"
    );
}
