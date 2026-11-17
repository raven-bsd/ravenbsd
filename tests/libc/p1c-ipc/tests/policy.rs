// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Host checks for the P1c-ipc-tty slice.
//!
//! The first test re-derives the claimed interface list and compiles every probe with the same
//! feature macros as `posix-check --host-only`: a row that is claimed `present` must be declared
//! under `_POSIX_C_SOURCE=202405L` and `_XOPEN_SOURCE=800`. The remaining tests pin the policy
//! answers (real `ftok`/speed logic, typed `ENOSYS`/`EBADF` for the primitives the kernel lacks).

use std::env;
use std::path::Path;
use std::process::Command;

use raven_libc_abi::structs::Termios;
use raven_libc_ipc::{
    EBADF, EINVAL, ENAMETOOLONG, ENOSYS, NAME_MAX, aio, mqueue, stropts, sysv, termios,
};

/// Every interface row P1c-ipc-tty claims, as `(header, symbol)`.
const INTERFACES: [(&str, &str); 59] = [
    ("aio.h", "aio_cancel"),
    ("aio.h", "aio_cancel64"),
    ("aio.h", "aio_error"),
    ("aio.h", "aio_error64"),
    ("aio.h", "aio_fsync"),
    ("aio.h", "aio_fsync64"),
    ("aio.h", "aio_read"),
    ("aio.h", "aio_read64"),
    ("aio.h", "aio_return"),
    ("aio.h", "aio_return64"),
    ("aio.h", "aio_suspend"),
    ("aio.h", "aio_suspend64"),
    ("aio.h", "aio_write"),
    ("aio.h", "aio_write64"),
    ("aio.h", "lio_listio"),
    ("aio.h", "lio_listio64"),
    ("mqueue.h", "mq_close"),
    ("mqueue.h", "mq_getattr"),
    ("mqueue.h", "mq_notify"),
    ("mqueue.h", "mq_open"),
    ("mqueue.h", "mq_receive"),
    ("mqueue.h", "mq_send"),
    ("mqueue.h", "mq_setattr"),
    ("mqueue.h", "mq_timedreceive"),
    ("mqueue.h", "mq_timedsend"),
    ("mqueue.h", "mq_unlink"),
    ("stropts.h", "fattach"),
    ("stropts.h", "fdetach"),
    ("stropts.h", "getmsg"),
    ("stropts.h", "getpmsg"),
    ("stropts.h", "isastream"),
    ("stropts.h", "putmsg"),
    ("stropts.h", "putpmsg"),
    ("termios.h", "cfgetispeed"),
    ("termios.h", "cfgetospeed"),
    ("termios.h", "cfsetspeed"),
    ("termios.h", "cfsetispeed"),
    ("termios.h", "cfsetospeed"),
    ("termios.h", "tcdrain"),
    ("termios.h", "tcflow"),
    ("termios.h", "tcflush"),
    ("termios.h", "tcgetsid"),
    ("termios.h", "tcgetwinsize"),
    ("termios.h", "tcsendbreak"),
    ("termios.h", "tcsetwinsize"),
    ("sys/ipc.h", "ftok"),
    ("sys/msg.h", "msgctl"),
    ("sys/msg.h", "msgget"),
    ("sys/msg.h", "msgrcv"),
    ("sys/msg.h", "msgsnd"),
    ("sys/sem.h", "semctl"),
    ("sys/sem.h", "semget"),
    ("sys/sem.h", "semop"),
    ("sys/sem.h", "semtimedop"),
    ("sys/shm.h", "shmat"),
    ("sys/shm.h", "shmctl"),
    ("sys/shm.h", "shmdt"),
    ("sys/shm.h", "shmget"),
    ("sys/shm.h", "shm_unlink"),
];

#[test]
fn every_claimed_interface_compiles_in_posix_and_xsi_mode() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let include = root.join("libc/ravenlibc/include");
    let arch = root.join("libc/ravenlibc/arch/x86_64/include");
    let output_dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    if Command::new(&cc).arg("--version").output().is_err() {
        eprintln!("skipped: `{cc}` is not available");
        return;
    }

    for (header, name) in INTERFACES {
        let source = format!(
            "#include <{header}>\nstatic void *const probe = (void *)&{name};\nint main(void) {{ return 0; }}\n"
        );
        let path = output_dir.join(format!("p1c_ipc_{name}.c"));
        std::fs::write(&path, source).expect("write the probe source");
        let output = Command::new(&cc)
            .arg("-fsyntax-only")
            .arg("-std=c17")
            .arg("-D_POSIX_C_SOURCE=202405L")
            .arg("-D_XOPEN_SOURCE=800")
            .arg("-Wno-unused-value")
            .arg("-I")
            .arg(&include)
            .arg("-I")
            .arg(&arch)
            .arg(&path)
            .output()
            .expect("run the C compiler");
        assert!(
            output.status.success(),
            "{header}: {name} did not compile:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn termios_speed_logic_is_real() {
    let mut record = Termios::ZERO;
    assert_eq!(termios::cfgetispeed(&record), termios::B0);
    assert_eq!(termios::cfsetispeed(&mut record, 0o15), Ok(()));
    assert_eq!(termios::cfgetispeed(&record), 0o15);
    assert_eq!(termios::cfsetospeed(&mut record, termios::B57600), Ok(()));
    assert_eq!(termios::cfgetospeed(&record), termios::B57600);
    assert_eq!(termios::cfsetspeed(&mut record, termios::B38400), Ok(()));
    assert_eq!(termios::cfgetispeed(&record), termios::B38400);
    assert_eq!(termios::cfgetospeed(&record), termios::B38400);
    assert_eq!(termios::cfsetispeed(&mut record, 0o20), Err(EINVAL));
    assert_eq!(termios::cfgetispeed(&record), termios::B38400);
}

#[test]
fn mqueue_names_and_attributes_are_validated() {
    assert!(mqueue::validate_name(b"/queue").is_ok());
    assert_eq!(mqueue::validate_name(b"queue"), Err(EINVAL));
    assert_eq!(mqueue::validate_name(b"/"), Err(EINVAL));
    let too_long = [b'a'; NAME_MAX + 1];
    let mut name = [b'/'; NAME_MAX + 2];
    name[1..].copy_from_slice(&too_long);
    assert_eq!(mqueue::validate_name(&name), Err(ENAMETOOLONG));
}

#[test]
fn ftok_is_derived_from_stat_data() {
    assert_eq!(sysv::ftok_key(0x1234, 0xbeef, 0x42), 0x4234_beef);
    assert!(sysv::key_is_private(sysv::IPC_PRIVATE));
}

#[test]
fn unavailable_primitives_answer_with_typed_errors() {
    assert_eq!(aio::unsupported(), ENOSYS);
    assert_eq!(mqueue::unsupported(), ENOSYS);
    assert_eq!(mqueue::queue_descriptor_error(), EBADF);
    assert_eq!(stropts::unsupported(), ENOSYS);
    assert_eq!(sysv::unsupported(), ENOSYS);
    assert_eq!(termios::unsupported(), ENOSYS);
    assert_eq!(stropts::isastream(-1), Err(EBADF));
    assert_eq!(stropts::isastream(0), Ok(false));
}
