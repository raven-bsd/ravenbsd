// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::Path;

use super::*;

#[test]
fn the_last_init_exit_wins() {
    let text = "init exited (1)\nmore output\ninit exited (0)\n";
    assert_eq!(parse_init_exit(text), Some(0));
}

#[test]
fn a_missing_init_exit_is_none() {
    assert_eq!(parse_init_exit("process: starting /init\npanic\n"), None);
}

#[test]
fn an_unparsable_init_exit_is_none() {
    assert_eq!(parse_init_exit("init exited (oops)\n"), None);
}

#[test]
fn a_matching_exit_passes() {
    let observation = judge("init exited (0)\n", Some(33), false, 0, 120);
    assert_eq!(observation.verdict, Verdict::Pass);
    assert!(observation.reason.is_empty());
}

#[test]
fn a_different_exit_fails_with_the_code() {
    let observation = judge("init exited (1)\n", Some(35), false, 0, 120);
    assert_eq!(observation.verdict, Verdict::Fail);
    assert_eq!(observation.reason, "behaviour:expected exit 0, got 1");
}

#[test]
fn a_non_zero_expectation_is_honoured() {
    let observation = judge("init exited (2)\n", Some(35), false, 2, 120);
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn a_timeout_is_a_failure() {
    let observation = judge("", None, true, 0, 7);
    assert_eq!(observation.verdict, Verdict::Fail);
    assert_eq!(observation.reason, "timeout after 7s");
}

#[test]
fn a_missing_init_exit_is_a_crash() {
    let observation = judge("loading\n", Some(35), false, 0, 120);
    assert_eq!(observation.verdict, Verdict::Fail);
    assert_eq!(observation.reason, "crash:no init exit (qemu exit 35)");
}

#[test]
fn mkinitfs_packs_the_program_as_init() {
    let args = mkinitfs_args(Path::new("/work/initfs.img"), Path::new("/out/program"));
    assert_eq!(args.last().expect("last"), "/init=/out/program");
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--example", "mkinitfs"])
    );
    assert!(args.windows(2).any(|pair| pair == ["--features", "std"]));
}

#[test]
fn the_image_installs_the_kernel_and_initfs() {
    let args = image_args(
        Path::new("/build/kernel.elf"),
        Path::new("/work/initfs.img"),
        Path::new("/work/boot.img"),
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--initfs", "/work/initfs.img"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--kernel", "/build/kernel.elf"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--output", "/work/boot.img"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--cmdline", "console=ttyS0 test-exit=1"])
    );
    assert!(args.windows(2).any(|pair| pair == ["--arch", "x86_64"]));
}

#[test]
fn qemu_uses_the_c5_exit_device_and_stdio_serial() {
    let args = qemu_args(
        Path::new("/work/boot.img"),
        Path::new("/code.fd"),
        Path::new("/vars.fd"),
    );
    assert!(args.windows(2).any(|pair| pair == ["-serial", "stdio"]));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["-drive", "format=raw,file=/work/boot.img"])
    );
    assert!(
        args.iter()
            .any(|arg| arg == "isa-debug-exit,iobase=0xf4,iosize=0x04")
    );
    assert!(args.iter().any(|arg| arg == "-no-reboot"));
    assert!(
        args.windows(2)
            .any(|pair| pair == ["-drive", "if=pflash,format=raw,file=/vars.fd"])
    );
}
