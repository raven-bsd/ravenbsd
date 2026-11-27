// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c unistd/IO wrapper tests.

use super::*;
use crate::backend::FakeSys;

fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.file(b"/a", b"x");
    fake.sys(b"kern/hostid", b"1234\n");
    fake.sys(b"kern/domainname", b"example\n");
    Platform::new(fake)
}

#[test]
fn chown_family_uses_the_root_handle() {
    let platform = platform();
    assert_eq!(platform.chown(b"/a", 1, 2), Ok(()));
    assert_eq!(platform.lchown(b"/a", 1, 2), Ok(()));
    let fd = platform.open(b"/a", 0, 0).expect("open");
    assert_eq!(platform.fchown(fd, 1, 2), Ok(()));
    let _ = platform.close(fd);
}

#[test]
fn dev_tty_opens_the_controlling_terminal_handle() {
    // `/dev/tty` names the process's controlling terminal, not a VFS node. The backend supplies
    // the live terminal handle; the public path stays on the OFD for `ttyname`.
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.ctty(Handle(0x1234));
    let platform = Platform::new(fake);
    let fd = platform.open(b"/dev/tty", 0, 0).expect("open /dev/tty");
    let reference = platform.get(fd).expect("fd");
    assert_eq!(reference.kind(), OfdKind::Tty);
    assert_eq!(reference.handle(), Handle(0x1234));
    assert_eq!(platform.close(fd), Ok(()));
}

#[test]
fn pty_slave_chown_and_chmod_are_read_only_device_metadata() {
    // The pty slave name is a `pty:` scheme endpoint, not a VFS node: the pty server owns the
    // node metadata and reports a root-owned 0600 chr device. OpenSSH `sshpty.c` `pty_setowner`
    // accepts `EROFS` for exactly that read-only, root-owned node; a literal VFS `Chown` here is
    // what made `ssh -tt` fatal at `chown(/dev/pts/1, 1000, 1000)`.
    let platform = platform();
    assert_eq!(platform.chown(b"/dev/pts/1", 1000, 1000), Err(Errno::EROFS));
    assert_eq!(platform.chmod(b"/dev/pts/1", 0o600), Err(Errno::EROFS));
    // An ordinary VFS path still goes through the root handle.
    assert_eq!(platform.chown(b"/a", 1, 2), Ok(()));
    assert_eq!(platform.chmod(b"/a", 0o644), Ok(()));
}

#[test]
fn mkfifoat_and_mknodat_resolve_through_the_cwd() {
    let platform = platform();
    assert_eq!(platform.mkfifoat(-100, b"fifo", 0o644), Ok(()));
    assert_eq!(platform.mknodat(-100, b"node", 0o600, 7), Ok(()));
}

#[test]
fn utime_sends_the_explicit_times() {
    let platform = platform();
    assert_eq!(platform.utime(b"/a", Some(10), Some(20)), Ok(()));
    assert_eq!(platform.utime(b"/a", None, Some(20)), Ok(()));
}

#[test]
fn getdtablesize_tracks_the_nofile_limit() {
    let platform = platform();
    assert_eq!(platform.getdtablesize(), 1024);
    platform.set_nofile_limits(256, 512);
    assert_eq!(platform.getdtablesize(), 256);
}

#[test]
fn exec_fdmap_env_carries_the_table_and_closes_cloexec() {
    use crate::fd::inherit::{ENV_NAME, FdMap};

    let platform = platform();
    let plain = platform.open(b"/a", 0, 0).expect("open");
    let cloexec = platform.open(b"/a", 0, 0).expect("open");
    platform.set_cloexec(cloexec, true).expect("cloexec");

    let entry = platform.exec_fdmap_env().expect("env").expect("some");
    let (name, value) = entry.split_at(ENV_NAME.len());
    assert_eq!(name, ENV_NAME);
    let value = value.strip_prefix(b"=").expect("separator");
    let map = FdMap::decode_env(value).expect("decode");
    assert!(map.entries.iter().any(|row| row.fd == plain));
    assert!(
        map.entries
            .iter()
            .any(|row| row.fd == cloexec && row.fd_flags & 1 != 0)
    );

    platform.close_cloexec_fds().expect("close");
    assert!(platform.get(cloexec).is_err());
    assert!(platform.get(plain).is_ok());
}

#[test]
fn exec_fdmap_env_excludes_the_closed_session_fds() {
    use crate::fd::inherit::{ENV_NAME, FdMap};

    // The no-TTY command child: fds 0/1/2 are the stdin read, stdout write and
    // stderr write ends; the session's parent-side channel ends are 12/13/15
    // (wfd 12, rfd 13, efd 15 in the OpenSSH DEBUG3 trace). OpenSSH
    // session.c::do_child closes every descriptor >= STDERR_FILENO + 1 before
    // the execve, so the exec FdMap must carry only 0/1/2.
    let platform = platform();
    let mut fds = Vec::new();
    for _ in 0..16 {
        fds.push(platform.open(b"/a", 0, 0).expect("open"));
    }
    assert_eq!(fds.first().copied(), Some(0));
    assert_eq!(fds.get(12).copied(), Some(12));
    assert_eq!(fds.get(13).copied(), Some(13));
    assert_eq!(fds.get(15).copied(), Some(15));

    platform.close_from(3).expect("closefrom");

    let entry = platform.exec_fdmap_env().expect("env").expect("some");
    let (name, value) = entry.split_at(ENV_NAME.len());
    assert_eq!(name, ENV_NAME);
    let map = FdMap::decode_env(value.strip_prefix(b"=").expect("separator")).expect("decode");
    let carried: Vec<i32> = map.entries.iter().map(|row| row.fd).collect();
    assert_eq!(carried, vec![0, 1, 2]);
}

#[test]
fn stdio_env_publishes_the_standard_fd_handles() {
    let platform = platform();
    let mut fds = Vec::new();
    for _ in 0..3 {
        fds.push(platform.open(b"/a", 0, 0).expect("open"));
    }
    let env = platform.stdio_env();
    assert_eq!(env.len(), 3);
    for (fd, name) in [
        (fds[0], b"RSH_FD0".as_slice()),
        (fds[1], b"RSH_FD1"),
        (fds[2], b"RSH_FD2"),
    ] {
        let row = env
            .iter()
            .find(|row| row.starts_with(name))
            .expect("standard fd row");
        let separator = row
            .iter()
            .position(|byte| *byte == b'=')
            .expect("separator");
        assert_eq!(&row[..separator], name);
        let handle = platform.get(fd).expect("fd").handle().0;
        assert_eq!(&row[separator + 1..], handle.to_string().as_bytes());
    }
    for fd in fds {
        let _ = platform.close(fd);
    }
}

#[test]
fn hostid_and_domainname_round_trip() {
    let platform = platform();
    assert_eq!(platform.gethostid(), 1234);
    let mut buffer = [0u8; 32];
    assert_eq!(platform.getdomainname(&mut buffer), Ok(()));
    assert_eq!(&buffer[..11], b"example\0\0\0\0");
    assert_eq!(platform.setdomainname(b"other"), Ok(()));
    let mut buffer = [0u8; 32];
    assert_eq!(platform.getdomainname(&mut buffer), Ok(()));
    assert_eq!(&buffer[..6], b"other\0");
}

#[test]
fn lseek_moves_the_fake_cursor() {
    let platform = platform();
    let fd = platform.open(b"/a", 0, 0).expect("open");
    assert_eq!(platform.lseek(fd, 0, 2), Ok(1));
    assert_eq!(platform.lseek(fd, -1, 1), Ok(0));
    assert_eq!(platform.lseek(fd, 3, 0), Ok(3));
    assert_eq!(platform.lseek(fd, 0, 1), Ok(3));
    assert_eq!(platform.lseek(fd, 0, 9), Err(Errno::EINVAL));
    assert_eq!(platform.lseek(99, 0, 0), Err(Errno::EBADF));
    let _ = platform.close(fd);
}

#[test]
fn exec_fdmap_duplicates_the_event_handle() {
    use crate::fd::inherit::{ENV_NAME, FdMap};

    let platform = platform();
    let fd = platform.open(b"/a", 0, 0).expect("open");
    let index = platform.get(fd).expect("fd").index();
    let event = platform
        .backend()
        .dup(platform.get(fd).expect("fd").handle(), 0)
        .expect("event stand-in");
    platform.set_ofd_event(index, event).expect("attach");
    let env = platform.exec_fdmap_env().expect("env").expect("some");
    let (_, value) = env.split_at(ENV_NAME.len());
    let value = value.strip_prefix(b"=").expect("separator");
    let map = FdMap::decode_env(value).expect("decode");
    let inherited = map.entries.iter().find(|entry| entry.fd == fd).expect("fd");
    assert_ne!(inherited.event, event.0, "child gets its own event handle");
    let _ = platform
        .backend()
        .handle_close(crate::Handle(inherited.event));
    let _ = platform.close(fd);
}

#[test]
fn a_blocking_socket_read_waits_and_retries_after_eagain() {
    let platform = platform();
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let index = platform.get(fd).expect("fd").index();
    platform.set_ofd_kind(index, OfdKind::Socket);
    platform
        .set_ofd_event(index, crate::Handle(9))
        .expect("event");
    platform
        .backend()
        .ready(crate::Handle(9), crate::poll::EV_SIGNALED);
    platform
        .backend()
        .fail_next(crate::backend::Fault::Read, Errno::EAGAIN);
    let mut buffer = [0u8; 4];
    assert_eq!(platform.read(fd, &mut buffer), Ok(1));
    assert_eq!(buffer[0], b'x');
    let _ = platform.close(fd);
}

#[test]
fn a_nonblocking_socket_read_returns_eagain() {
    let platform = platform();
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let index = platform.get(fd).expect("fd").index();
    platform.set_ofd_kind(index, OfdKind::Socket);
    platform.set_ofd_status(index, fcntl::O_NONBLOCK as u32);
    platform
        .backend()
        .fail_next(crate::backend::Fault::Read, Errno::EAGAIN);
    let mut buffer = [0u8; 4];
    assert_eq!(platform.read(fd, &mut buffer), Err(Errno::EAGAIN));
    let _ = platform.close(fd);
}

#[test]
fn closing_an_fd_releases_its_doorbell_with_handle_close() {
    let platform = platform();
    let base = platform.backend().open_handles();
    let fd = platform.open(b"/a", 0, 0).expect("open");
    let doorbell = platform
        .backend()
        .dup(platform.get(fd).expect("fd").handle(), 0)
        .expect("a second handle stands in for the doorbell");
    let index = platform.get(fd).expect("fd").index();
    platform.set_ofd_event(index, doorbell).expect("attach");
    assert_eq!(
        platform.backend().open_handles(),
        base + 2,
        "data + doorbell"
    );
    assert_eq!(platform.close(fd), Ok(()));
    assert_eq!(
        platform.backend().handle_close_calls(),
        1,
        "the doorbell goes through HANDLE_CLOSE"
    );
    assert_eq!(platform.backend().open_handles(), base, "nothing leaks");
}

#[test]
fn fork_child_keeps_the_inherited_pty_master_handle() {
    let platform = platform();
    let file = platform.open(b"/a", 0, 0).expect("file");
    let handle = platform.get(file).expect("file").handle();
    let pty = platform
        .install(handle, OfdKind::Tty, 2, None, false)
        .expect("pty");
    let old_pty = platform.get(pty).expect("pty").handle();
    let index = platform.get(pty).expect("pty").index();

    platform.fork_child_inherit();

    // The kernel already cloned the handle into the child and the SchemeFile Arc tracks both
    // descriptors; a DUP would add a second server reference that only the child's process exit
    // releases, deferring the last-master CLOSE (and SIGHUP) until the foreground child is gone.
    assert_eq!(
        platform.get(pty).expect("pty").handle(),
        old_pty,
        "the pty shares the kernel's single server reference across fork"
    );
    assert_eq!(
        platform.ofd_handle(index),
        Some(old_pty),
        "the OFD keeps the inherited pty handle"
    );
}

#[test]
fn fork_child_pipe_close_keeps_the_parents_read_end_open() {
    let platform = platform();
    let file = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("file");
    let file_handle = platform.get(file).expect("file").handle();
    let pipe_handle = platform.backend().dup(file_handle, 0).expect("pipe handle");
    let pipe = platform
        .install(
            pipe_handle,
            OfdKind::Pipe,
            fcntl::O_RDONLY as u32,
            None,
            false,
        )
        .expect("pipe");
    let old_pipe = platform.get(pipe).expect("pipe").handle();
    let index = platform.get(pipe).expect("pipe").index();

    platform.fork_child_inherit();

    let new_pipe = platform.get(pipe).expect("pipe").handle();
    assert_ne!(
        new_pipe, old_pipe,
        "the child takes a fresh scheme handle for the pipe"
    );
    assert_eq!(
        platform.ofd_handle(index),
        Some(new_pipe),
        "the OFD is the authority for the live pipe handle"
    );

    // The child closes its duplicate first; the parent's read end must still be open.
    platform.close(pipe).expect("child close");
    let mut byte = [0u8; 1];
    assert_eq!(
        platform.backend().read(old_pipe, &mut byte),
        Ok(1),
        "the parent pipe read stays open after the child close"
    );

    // The last close releases the pipe: a further read sees no handle (EOF at the OFD level).
    platform.backend().close(old_pipe).expect("parent close");
    assert_eq!(
        platform.backend().read(old_pipe, &mut byte),
        Err(Errno::EBADF)
    );
}

#[test]
fn fork_child_duplicates_an_inherited_socket_handle() {
    let platform = platform();
    let file = platform.open(b"/a", 0, 0).expect("file");
    let handle = platform.get(file).expect("file").handle();
    let socket = platform
        .install(handle, OfdKind::Socket, 2, None, false)
        .expect("socket");
    let old_socket = platform.get(socket).expect("socket").handle();
    let old_file = platform.get(file).expect("file").handle();
    let index = platform.get(socket).expect("socket").index();

    platform.fork_child_inherit();

    let new_socket = platform.get(socket).expect("socket").handle();
    assert_ne!(
        new_socket, old_socket,
        "the child takes a fresh scheme handle"
    );
    assert_eq!(
        platform.ofd_handle(index),
        Some(new_socket),
        "the OFD is the authority for the live handle"
    );
    assert_eq!(
        platform.get(socket).expect("socket").index(),
        index,
        "the fd keeps the same OFD"
    );
    assert_eq!(
        platform.get(file).expect("file").handle(),
        old_file,
        "a non-netd file keeps its inherited handle"
    );
}
