// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for stack layout, initfs spawning and file-image spawning.

use super::*;
use raven_abi::layout::x86_64::USER_END;
use std::cell::RefCell;

const TEST_TOP: u64 = 0x8000_0000;
const TEST_BUFFER: usize = 64 * 1024;

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
        auxv,
        &[0xAB; RANDOM_LEN],
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
            &[],
            &[0; 16]
        ),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn too_many_arguments_are_too_large() {
    let names: Vec<&str> = vec![""; 9000];
    let mut buffer = vec![0u8; TEST_BUFFER];
    assert!(matches!(
        build_stack(&mut buffer, TEST_TOP, "/child", &names, &[], &[], &[0; 16]),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn misaligned_stack_top_is_rejected() {
    let mut buffer = vec![0u8; TEST_BUFFER];
    assert!(matches!(
        build_stack(&mut buffer, TEST_TOP + 1, "/child", &[], &[], &[], &[0; 16]),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn small_buffer_is_rejected() {
    let mut buffer = [0u8; 64];
    assert!(matches!(
        build_stack(&mut buffer, TEST_TOP, "/child", &["a"], &[], &[], &[0; 16]),
        Err(SpawnError::TooLarge)
    ));
}

#[test]
fn image_only_touches_the_top_of_the_buffer() {
    let mut buffer = vec![0xFFu8; TEST_BUFFER];
    let (offset, _) =
        build_stack(&mut buffer, TEST_TOP, "/child", &["a"], &[], &[], &[0; 16]).unwrap();
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
    HandleClose(u32),
    WriteParentMem {
        addr: usize,
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
    assert_eq!(calls.len(), 9, "{calls:?}");
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
    assert_eq!(
        flags,
        map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED)
    );
    assert_eq!(
        calls[6],
        Call::Unmap {
            proc: Handle::SELF_PROC.0,
            addr: SCRATCH,
            len: STACK_SIZE,
        }
    );
    assert_eq!(calls[7], Call::HandleClose(MEMOBJ.0));
    assert!(matches!(
        &calls[8],
        Call::ThreadCreate {
            proc,
            entry: 0x0040_0100,
            stack_top,
            arg
        } if *proc == CHILD.0 && stack_top == arg && *stack_top >= stack_base
    ));
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
fn interpreter_is_rejected() {
    let image = initfs_with(&child_elf(true, false));
    let initfs = Initfs::parse(&image).unwrap();
    let mut mock = Mock::default();
    let error = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap_err();
    assert_eq!(error, SpawnError::HasInterpreter);
    assert!(mock.calls().is_empty());
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
    let long = "x".repeat(70_000);
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
