// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FakeSys and `Platform` lifecycle tests.

use alloc::vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_libc_abi::fcntl;

use crate::backend::{FakeSys, Fault, SysBackend};
use crate::platform::Platform;

#[test]
fn fake_file_round_trip() {
    let fake = FakeSys::new();
    fake.file(b"/etc/passwd", b"root:x:0:0\n");
    let handle = fake
        .open(
            b"file:/etc/passwd",
            u32::from(raven_proto_fs::WIRE_OPEN_READ),
        )
        .expect("open");
    let mut buffer = [0u8; 32];
    let count = fake.read(handle, &mut buffer).expect("read");
    assert_eq!(&buffer[..count], b"root:x:0:0\n");
}

#[test]
fn fake_open_missing_is_enoent() {
    let fake = FakeSys::new();
    assert_eq!(
        fake.open(b"file:/missing", u32::from(raven_proto_fs::WIRE_OPEN_READ)),
        Err(Errno::ENOENT)
    );
}

#[test]
fn fake_faults_are_one_shot() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    fake.fail_next(Fault::Open, Errno::EIO);
    assert_eq!(
        fake.open(b"file:/a", u32::from(raven_proto_fs::WIRE_OPEN_READ)),
        Err(Errno::EIO)
    );
    assert!(
        fake.open(b"file:/a", u32::from(raven_proto_fs::WIRE_OPEN_READ))
            .is_ok()
    );
}

#[test]
fn fake_dup_shares_the_cursor() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"abcdef");
    let first = fake
        .open(b"file:/a", u32::from(raven_proto_fs::WIRE_OPEN_READ))
        .expect("open");
    let second = fake.dup(first, 0).expect("dup");
    let mut one = [0u8; 2];
    let mut two = [0u8; 2];
    assert_eq!(fake.read(first, &mut one), Ok(2));
    assert_eq!(fake.read(second, &mut two), Ok(2));
    assert_eq!(&one, b"ab");
    assert_eq!(&two, b"cd");
}

#[test]
fn fake_write_and_contents() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"");
    let handle = fake
        .open(b"file:/a", u32::from(raven_proto_fs::WIRE_OPEN_WRITE))
        .expect("open");
    assert_eq!(fake.write(handle, b"hello"), Ok(5));
    assert_eq!(fake.contents(b"/a").as_deref(), Some(b"hello".as_slice()));
}

#[test]
fn fake_fstat_decodes() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"12345");
    let handle = fake
        .open(b"file:/a", u32::from(raven_proto_fs::WIRE_OPEN_READ))
        .expect("open");
    let mut record = [0u8; 64];
    fake.fstat(handle, &mut record).expect("fstat");
    let stat = raven_proto_fs::Stat::decode(&record).expect("decode");
    assert_eq!(stat.size, 5);
    assert_eq!(stat.blocks, 1);
}

#[test]
fn fake_statfs_call() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let handle = fake
        .open(
            b"file:/",
            u32::from(raven_proto_fs::WIRE_OPEN_READ | raven_proto_fs::WIRE_OPEN_DIRECTORY),
        )
        .expect("open");
    let mut request = [0u8; 16];
    let encoded = raven_proto_fs::CallRequest::new(raven_proto_fs::Call::Statfs)
        .encode(&mut request)
        .expect("encode");
    let mut response = [0u8; 64];
    let count = fake
        .call(handle, &request[..encoded], &mut response)
        .expect("call");
    let decoded =
        raven_proto_fs::CallResponse::decode(raven_proto_fs::OP_STATFS, &response[..count])
            .expect("decode");
    match decoded {
        raven_proto_fs::CallResponse::StatFs { statfs, .. } => {
            assert_eq!(statfs.block_size, 4096);
        }
        _ => panic!("wrong response"),
    }
}

#[test]
fn fake_map_is_zero_filled() {
    let fake = FakeSys::new();
    let address = fake.map(Handle::NONE, 0, 0, 8192, 3, 0).expect("map");
    assert_eq!(fake.mapped(address).expect("mapped"), vec![0u8; 8192]);
    assert!(fake.poke(address, 10, b"abc"));
    let mapped = fake.mapped(address).expect("mapped");
    assert_eq!(&mapped[10..13], b"abc");
}

#[test]
fn fake_uname_and_clock() {
    let fake = FakeSys::new();
    let uname = fake.uname().expect("uname");
    assert_eq!(
        raven_abi::uname::Uname::field_str(&uname.sysname),
        Some("RavenBSD")
    );
    let before = fake.clock_get(1).expect("clock").tv_nsec;
    fake.advance_ns(1_000_000);
    let after = fake.clock_get(1).expect("clock");
    assert!(after.tv_nsec > before);
}

#[test]
fn platform_open_read_close() {
    let fake = FakeSys::new();
    fake.file(b"/tmp/x", b"payload");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/tmp/x", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let mut buffer = [0u8; 16];
    let count = platform.read(fd, &mut buffer).expect("read");
    assert_eq!(&buffer[..count], b"payload");
    assert_eq!(platform.close(fd), Ok(()));
    assert_eq!(platform.close(fd), Err(Errno::EBADF));
    assert_eq!(platform.open_count(), 0);
    assert_eq!(platform.ofd_count(), 0);
}

#[test]
fn platform_dup_shares_the_ofd() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"abcdef");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let duplicate = platform.dup(fd, 0, true).expect("dup");
    assert_eq!(duplicate, 1);
    assert!(platform.cloexec(duplicate).expect("cloexec"));
    let mut one = [0u8; 2];
    let mut two = [0u8; 2];
    assert_eq!(platform.read(fd, &mut one), Ok(2));
    assert_eq!(platform.read(duplicate, &mut two), Ok(2));
    assert_eq!(&one, b"ab");
    assert_eq!(&two, b"cd", "the cursor is shared");
    assert_eq!(platform.ofd_count(), 1);
    assert_eq!(platform.ofd_refs(), 2);
}

#[test]
fn platform_dup2_replaces_and_drops_only_at_zero() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"a");
    fake.file(b"/b", b"b");
    let platform = Platform::new(fake);
    let a = platform.open(b"/a", fcntl::O_RDONLY as u32, 0).expect("a");
    let b = platform.open(b"/b", fcntl::O_RDONLY as u32, 0).expect("b");
    assert_eq!((a, b), (0, 1));
    assert_eq!(platform.dup2(a, b, false), Ok(b));
    assert_eq!(platform.ofd_count(), 1, "the displaced OFD is gone");
    assert_eq!(platform.ofd_refs(), 2);
    assert_eq!(platform.dup2(b, b, false), Ok(b));
}

#[test]
fn platform_blocked_reader_keeps_the_ofd_alive() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let reference = platform.get(fd).expect("reference");
    assert_eq!(platform.close(fd), Ok(()));
    assert_eq!(platform.ofd_count(), 1, "the reader still holds it");
    assert_eq!(reference.handle().0, 5);
    drop(reference);
    assert_eq!(platform.ofd_count(), 0);
}

#[test]
fn platform_open_write_only_read_is_ebadf() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_WRONLY as u32, 0)
        .expect("open");
    let mut buffer = [0u8; 1];
    assert_eq!(platform.read(fd, &mut buffer), Err(Errno::EBADF));
}

#[test]
fn platform_read_only_write_is_ebadf() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(platform.write(fd, b"y"), Err(Errno::EBADF));
}

#[test]
fn platform_fcntl_round_trips_flags() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(
            b"/a",
            (fcntl::O_RDWR | fcntl::O_APPEND | fcntl::O_NONBLOCK) as u32,
            0,
        )
        .expect("open");
    let flags = platform.fcntl(fd, fcntl::F_GETFL, 0).expect("getfl");
    assert_eq!(
        flags as u32 & fcntl::O_APPEND as u32,
        fcntl::O_APPEND as u32
    );
    assert_eq!(
        flags as u32 & fcntl::O_NONBLOCK as u32,
        fcntl::O_NONBLOCK as u32
    );
    platform
        .fcntl(fd, fcntl::F_SETFL, i64::from(fcntl::O_APPEND))
        .expect("setfl");
    let flags = platform.fcntl(fd, fcntl::F_GETFL, 0).expect("getfl");
    assert_eq!(flags as u32 & fcntl::O_NONBLOCK as u32, 0);
}

#[test]
fn platform_fcntl_dupfd_limits() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(platform.fcntl(fd, fcntl::F_DUPFD, 1024), Err(Errno::EINVAL));
    let duplicate = platform.fcntl(fd, fcntl::F_DUPFD, 10).expect("dupfd");
    assert_eq!(duplicate, 10);
}

#[test]
fn platform_stat_converts_the_record() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"hello");
    let platform = Platform::new(fake);
    let stat = platform.stat(b"/a").expect("stat");
    assert_eq!(stat.st_size, 5);
    assert_eq!(stat.st_blksize, 4096);
    assert_eq!(stat.st_blocks, 1);
    assert_eq!(
        stat.st_mode & raven_libc_abi::stat::S_IFMT,
        raven_libc_abi::stat::S_IFREG
    );
}

#[test]
fn platform_stat_missing_is_enoent() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(platform.stat(b"/missing"), Err(Errno::ENOENT));
}

#[test]
fn platform_chdir_into_a_file_is_enotdir() {
    let fake = FakeSys::new();
    fake.file(b"/f", b"");
    let platform = Platform::new(fake);
    assert_eq!(platform.chdir(b"/f"), Err(Errno::ENOTDIR));
}

#[test]
fn platform_chdir_into_a_directory_works() {
    let fake = FakeSys::new();
    fake.dir(b"/tmp");
    let platform = Platform::new(fake);
    platform.chdir(b"/tmp").expect("chdir");
    assert_eq!(platform.cwd(), b"/tmp");
    assert_eq!(
        platform.open(b"x", fcntl::O_RDONLY as u32, 0),
        Err(Errno::ENOENT)
    );
}

#[test]
fn platform_mkdir_rmdir_unlink_rename() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.dir(b"/dir");
    fake.file(b"/dir/a", b"");
    let platform = Platform::new(fake);
    platform.mkdir(b"/dir/sub", 0o755).expect("mkdir");
    platform.rename(b"/dir/a", b"/dir/b").expect("rename");
    platform.unlink(b"/dir/b").expect("unlink");
    platform.rmdir(b"/dir/sub").expect("rmdir");
}

#[test]
fn platform_unlink_trailing_slash_is_eisdir() {
    let fake = FakeSys::new();
    fake.dir(b"/d");
    let platform = Platform::new(fake);
    assert_eq!(platform.unlink(b"/d/"), Err(Errno::EISDIR));
}

#[test]
fn platform_mkdir_applies_umask() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let platform = Platform::new(fake);
    assert_eq!(platform.set_umask(0o022), 0o022);
    platform.mkdir(b"/x", 0o777).expect("mkdir");
    assert!(platform.stat(b"/x").is_ok());
}

#[test]
fn platform_close_from_and_range() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"");
    let platform = Platform::new(fake);
    for _ in 0..5 {
        platform
            .open(b"/a", fcntl::O_RDONLY as u32, 0)
            .expect("open");
    }
    platform.close_range(1, 3).expect("range");
    assert_eq!(platform.open_count(), 3);
    platform.close_from(1).expect("from");
    assert_eq!(platform.open_count(), 1);
}

#[test]
fn platform_pathconf_needs_the_path() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"");
    let platform = Platform::new(fake);
    assert_eq!(
        platform.pathconf(b"/missing", raven_libc_abi::sysconf::_PC_NAME_MAX),
        Err(Errno::ENOENT)
    );
    assert_eq!(
        platform.pathconf(b"/a", raven_libc_abi::sysconf::_PC_NAME_MAX),
        Ok(255)
    );
}

#[test]
fn platform_statvfs_round_trip() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let platform = Platform::new(fake);
    let statvfs = platform.fstatvfs(0).unwrap_or_else(|_| {
        let fd = platform
            .open(b"/", (fcntl::O_RDONLY | fcntl::O_DIRECTORY) as u32, 0)
            .expect("open");
        platform.fstatvfs(fd).expect("fstatvfs")
    });
    assert_eq!(statvfs.f_bsize, 4096);
    assert_eq!(statvfs.f_namemax, 255);
}

#[test]
fn inheritance_block_round_trip_from_a_platform() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let reference = platform.get(fd).expect("ref");
    let block = platform
        .build_fdmap(&[(reference.handle(), Handle(0x99))])
        .expect("map");
    drop(reference);
    assert_eq!(block.entries.len(), 1);
    assert_eq!(block.entries[0].handle, 0x99);
    let bytes = block.encode().expect("encode");
    let decoded = crate::fd::inherit::FdMap::decode(&bytes).expect("decode");
    assert_eq!(decoded.entries[0].path.as_deref(), Some(b"/a".as_slice()));
}

#[test]
fn platform_open_create_applies_mode_and_umask() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/new", (fcntl::O_RDWR | fcntl::O_CREAT) as u32, 0o666)
        .expect("create");
    assert!(platform.backend().exists(b"/new"));
    let stat = platform.fstat(fd).expect("fstat");
    assert_eq!(stat.st_mode & 0o777, 0o644);
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn platform_open_excl_on_existing_is_eexist() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.file(b"/new", b"");
    let platform = Platform::new(fake);
    assert_eq!(
        platform.open(
            b"/new",
            (fcntl::O_RDWR | fcntl::O_CREAT | fcntl::O_EXCL) as u32,
            0o666
        ),
        Err(Errno::EEXIST)
    );
}

#[test]
fn platform_open_trunc_clears_the_file() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"payload");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", (fcntl::O_RDWR | fcntl::O_TRUNC) as u32, 0)
        .expect("trunc");
    assert_eq!(platform.fstat(fd).expect("fstat").st_size, 0);
}

#[test]
fn platform_open_creat_with_trailing_slash_is_eisdir() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let platform = Platform::new(fake);
    assert_eq!(
        platform.open(b"/new/", (fcntl::O_WRONLY | fcntl::O_CREAT) as u32, 0o666),
        Err(Errno::EISDIR)
    );
}

#[test]
fn platform_mkdir_with_trailing_slash_succeeds() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    let platform = Platform::new(fake);
    platform.mkdir(b"/dir/", 0o755).expect("mkdir");
    assert!(platform.stat(b"/dir").is_ok());
}

#[test]
fn platform_rename_file_with_trailing_slash_is_enotdir() {
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    assert_eq!(platform.rename(b"/a/", b"/b"), Err(Errno::ENOTDIR));
}

#[test]
fn platform_open_trailing_slash_on_a_file_is_enotdir() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    assert_eq!(
        platform.open(b"/a/", fcntl::O_RDONLY as u32, 0),
        Err(Errno::ENOTDIR)
    );
}

#[test]
fn platform_stat_trailing_slash_on_a_file_is_enotdir() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    assert_eq!(platform.stat(b"/a/"), Err(Errno::ENOTDIR));
}

#[test]
fn platform_read_short_count() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"abc");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let mut buffer = [0u8; 16];
    assert_eq!(platform.read(fd, &mut buffer), Ok(3));
    assert_eq!(&buffer[..3], b"abc");
    assert_eq!(platform.read(fd, &mut buffer), Ok(0));
}

#[test]
fn platform_pread_is_enosys_and_pipe_is_espipe() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let mut buffer = [0u8; 1];
    assert_eq!(platform.pread(fd, &mut buffer, 0), Err(Errno::ENOSYS));
    assert_eq!(platform.lseek(fd, 0, 0), Err(Errno::ENOSYS));
    let pipe = platform
        .install(
            platform
                .backend()
                .dup(platform.get(fd).expect("ref").handle(), 0)
                .expect("dup"),
            crate::fd::ofd::OfdKind::Pipe,
            0,
            None,
            false,
        )
        .expect("install");
    assert_eq!(platform.pread(pipe, &mut buffer, 0), Err(Errno::ESPIPE));
    assert_eq!(platform.lseek(pipe, 0, 0), Err(Errno::ESPIPE));
}

#[test]
fn platform_ftruncate_needs_write_access() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(platform.ftruncate(fd, 0), Err(Errno::EINVAL));
}

#[test]
fn platform_readv_coalesces() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"abcdef");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let mut first = [0u8; 2];
    let mut second = [0u8; 2];
    let mut bufs: [&mut [u8]; 2] = [&mut first, &mut second];
    assert_eq!(platform.readv(fd, &mut bufs), Ok(4));
    assert_eq!(&first, b"ab");
    assert_eq!(&second, b"cd");
}

#[test]
fn platform_writev_preserves_order() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_WRONLY as u32, 0)
        .expect("open");
    assert_eq!(platform.writev(fd, &[b"ab", b"cd"]), Ok(4));
    assert_eq!(
        platform.backend().contents(b"/a").as_deref(),
        Some(b"abcd".as_slice())
    );
}

#[test]
fn platform_readv_empty_is_zero() {
    let platform = Platform::new(FakeSys::new());
    let mut bufs: [&mut [u8]; 0] = [];
    assert_eq!(platform.readv(0, &mut bufs), Ok(0));
}

#[test]
fn platform_getcwd_argument_cases() {
    let fake = FakeSys::new();
    fake.dir(b"/tmp");
    let platform = Platform::new(fake);
    platform.chdir(b"/tmp").expect("chdir");
    let mut buffer = [0u8; 5];
    assert_eq!(platform.getcwd(Some(&mut buffer), 5), Ok(None));
    assert_eq!(&buffer, b"/tmp\0");
    let allocated = platform.getcwd(None, 0).expect("alloc").expect("buffer");
    assert_eq!(allocated, b"/tmp\0");
    assert_eq!(platform.getcwd(Some(&mut buffer), 0), Err(Errno::EINVAL));
}

#[test]
fn adopt_probes_a_tty_handle() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let handle = fake
        .open(b"file:/dev/tty", u32::from(raven_proto_fs::WIRE_OPEN_READ))
        .expect("open");
    let platform = Platform::new(fake);
    let fd = platform.adopt(handle, 0, None, true).expect("adopt");
    assert_eq!(
        platform.get(fd).expect("ref").kind(),
        crate::fd::ofd::OfdKind::Tty
    );
    assert!(platform.cloexec(fd).expect("cloexec"));
}

#[test]
fn init_fdsan_reads_the_level() {
    let platform = Platform::new(FakeSys::new());
    platform.init_fdsan(Some(b"fatal"));
    assert_eq!(
        platform.fdsan().level(),
        crate::fd::fdsan::ErrorLevel::Fatal
    );
    platform.init_fdsan(None);
    assert_eq!(
        platform.fdsan().level(),
        crate::fd::fdsan::ErrorLevel::WarnAlways
    );
}

#[test]
fn statvfs_works_on_a_regular_file() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    assert!(platform.statvfs(b"/a").is_ok());
}

#[test]
fn platform_init_cwd_uses_an_absolute_pwd() {
    let platform = Platform::new(FakeSys::new());
    platform.init_cwd(Some(b"/usr/local"));
    assert_eq!(platform.cwd(), b"/usr/local");
    let platform = Platform::new(FakeSys::new());
    platform.init_cwd(Some(b"relative"));
    assert_eq!(platform.cwd(), b"/");
}

#[test]
fn platform_dup2_close_stress_is_leak_free() {
    use std::sync::{Arc, Mutex};

    struct State {
        platform: Platform<FakeSys>,
        fds: Vec<i32>,
        seed: u64,
    }

    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let state = Arc::new(Mutex::new(State {
        platform: Platform::new(fake),
        fds: Vec::new(),
        seed: 0x1234_5678_9abc_def0,
    }));
    let mut threads = Vec::new();
    for _ in 0..8 {
        let state = Arc::clone(&state);
        threads.push(std::thread::spawn(move || {
            for _ in 0..1250 {
                let mut guard = state.lock().expect("lock");
                let State {
                    platform,
                    fds,
                    seed,
                } = &mut *guard;
                *seed ^= seed.wrapping_shl(13);
                *seed ^= seed.wrapping_shr(7);
                *seed ^= seed.wrapping_shl(17);
                let action = *seed % 3;
                match action {
                    0 => {
                        if let Ok(fd) = platform.open(b"/a", fcntl::O_RDONLY as u32, 0) {
                            fds.push(fd);
                        }
                    }
                    1 if !fds.is_empty() => {
                        let index = (*seed as usize) % fds.len();
                        let source = fds[index];
                        let target = ((*seed >> 32) as i32) % 64;
                        if platform.dup2(source, target, false).is_ok() && !fds.contains(&target) {
                            fds.push(target);
                        }
                    }
                    _ if !fds.is_empty() => {
                        let index = (*seed as usize) % fds.len();
                        let fd = fds.swap_remove(index);
                        let _ = platform.close(fd);
                    }
                    _ => {}
                }
            }
        }));
    }
    for thread in threads {
        thread.join().expect("join");
    }
    let mut guard = state.lock().expect("lock");
    let fds = core::mem::take(&mut guard.fds);
    for fd in fds {
        let _ = guard.platform.close(fd);
    }
    assert_eq!(guard.platform.open_count(), 0, "no fd leaked");
    assert_eq!(guard.platform.ofd_refs(), 0, "no OFD reference leaked");
    assert_eq!(guard.platform.ofd_count(), 0, "no OFD slot leaked");
}

#[test]
fn fd_table_stress_keeps_refcounts_consistent() {
    use std::sync::{Arc, Mutex};

    use crate::fd::ofd::{OfdArena, OfdKind};
    use crate::fd::table::FdTable;

    struct State {
        table: FdTable,
        arena: OfdArena,
        seed: u64,
        next_handle: u32,
    }

    let state = Arc::new(Mutex::new(State {
        table: FdTable::new(256, 256),
        arena: OfdArena::new(),
        seed: 0x9e37_79b9_7f4a_7c15,
        next_handle: 10,
    }));
    let mut threads = Vec::new();
    for _ in 0..8 {
        let state = Arc::clone(&state);
        threads.push(std::thread::spawn(move || {
            for _ in 0..1250 {
                let mut guard = state.lock().expect("lock");
                let State {
                    table,
                    arena,
                    seed,
                    next_handle,
                } = &mut *guard;
                *seed ^= seed.wrapping_shl(13);
                *seed ^= seed.wrapping_shr(7);
                *seed ^= seed.wrapping_shl(17);
                let action = *seed % 3;
                let open: Vec<i32> = table.open_entries().iter().map(|(fd, _)| *fd).collect();
                match (action, open.first()) {
                    (0, _) => {
                        if let Ok(fd) = table.alloc(0) {
                            let index = arena.insert(Handle(*next_handle), OfdKind::File, 0, None);
                            *next_handle = next_handle.saturating_add(1);
                            table.install_at(fd, index, false, 0).expect("install");
                        }
                    }
                    (1, Some(fd)) => {
                        if let Some(entry) = table.remove(*fd)
                            && arena.drop_ref(entry.ofd)
                        {
                            let _ = arena.release(entry.ofd);
                        }
                    }
                    (_, Some(fd)) => {
                        let index = table.entry(*fd).expect("entry").ofd;
                        if let Ok(new_fd) = table.alloc(0) {
                            arena.add_ref(index).expect("ref");
                            table.install_at(new_fd, index, false, 0).expect("install");
                        }
                    }
                    (_, None) => {}
                }
            }
        }));
    }
    for thread in threads {
        thread.join().expect("join");
    }
    let guard = state.lock().expect("lock");
    assert_eq!(
        guard.table.open_count() as u32,
        guard.arena.total_refs(),
        "one reference per installed fd"
    );
    assert!(guard.arena.live() <= guard.table.open_count());
}
