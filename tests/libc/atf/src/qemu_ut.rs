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
fn guest_markers_round_trip() {
    let text = "boot\nG1-OUTPUT-BEGIN\nhello\nworld\nG1-OUTPUT-END\nG1-EXIT 3\n";
    assert_eq!(marker_int(text, "G1-EXIT "), Some(3));
    assert_eq!(
        marker_block(text, "G1-OUTPUT-BEGIN", "G1-OUTPUT-END"),
        Some(b"hello\nworld".to_vec())
    );
    assert_eq!(
        marker_string("G1-RESULT-ERROR open failed\n", "G1-RESULT-ERROR "),
        Some("open failed".to_string())
    );
    let crlf = "G1-RESULT-BEGIN\r\npassed\r\nG1-RESULT-END\r\n";
    assert_eq!(
        marker_block(crlf, "G1-RESULT-BEGIN", "G1-RESULT-END"),
        Some(b"passed".to_vec())
    );
}

#[test]
fn the_guest_manifest_carries_argv_env_and_result() {
    let text = guest_manifest(
        GuestMode::Exec,
        &["hello world".to_string(), "-x".to_string()],
        &["TZ=UTC".to_string()],
        Some("/tmp/result"),
    );
    assert!(text.contains("mode=exec\n"));
    assert!(text.contains("program=/tests/program\n"));
    assert!(text.contains("arg=/tests/program\n"));
    assert!(text.contains("arg=hello world\n"));
    assert!(text.contains("arg=-x\n"));
    assert!(text.contains("env=TZ=UTC\n"));
    assert!(text.contains("result=/tmp/result\n"));
}

#[test]
fn the_guest_initfs_has_the_services_and_the_program() {
    let config = QemuConfig::new(Path::new("/repo"), 60);
    let guest = Guest::new(Path::new("/repo"));
    let workdir = std::env::temp_dir().join(format!("raven-atf-qemu-ut-{}", std::process::id()));
    let manifest = workdir.join("manifest");
    let specs = guest_initfs_specs(
        &config,
        &guest,
        Path::new("/out/program"),
        &manifest,
        Path::new("/work/g1.toml"),
        None,
        Path::new("/work/svcd.conf"),
    )
    .expect("guest initfs specs");
    let _ = std::fs::remove_dir_all(&workdir);
    assert!(
        specs
            .iter()
            .any(|spec| spec == "/init=/repo/build/fixtures/g1/guest/bin/init")
    );
    assert!(
        specs
            .iter()
            .any(|spec| spec == "/tests/program=/out/program:0755")
    );
    assert!(specs.iter().any(|spec| {
        spec == &format!(
            "/etc/services.d/vfsd.toml={}",
            workdir.join("etc/services-vfsd.toml").display()
        )
    }));
    assert!(
        specs
            .iter()
            .any(|spec| { spec == &format!("/g1/manifest={}", manifest.display()) })
    );
}

#[test]
fn the_batch_manifest_uses_the_indexed_image_and_result() {
    let text = batch_manifest("a_case", "/tests/t_foo", "/tmp/result-7", None);
    assert!(text.contains("mode=exec\n"));
    assert!(text.contains("program=/tests/t_foo\n"));
    assert!(text.contains("arg=-r\narg=/tmp/result-7\n"));
    assert!(text.contains("arg=-s\narg=/tests\narg=a_case\n"));
    assert!(text.contains("result=/tmp/result-7\n"));
}

#[test]
fn the_batch_manifest_carries_the_unprivileged_user() {
    let text = batch_manifest(
        "link_perm",
        "/tests/t_link",
        "/tmp/result-2",
        Some("unprivileged"),
    );
    assert!(text.contains("user=65534\n"));
    let root = batch_manifest("mkdir_err", "/tests/t_mkdir", "/tmp/result-3", Some("root"));
    assert!(!root.contains("user="));
}

#[test]
fn the_batch_parser_reads_exit_and_result_by_index() {
    let text = "boot\n\
G1-RUN 0\n\
process: child exited (0)\n\
G1-RESULT-BEGIN 0\n\
passed\n\
G1-RESULT-END 0\n\
G1-RUN 1\n\
process: child exited (134)\n\
G1-RESULT-MISSING 1 run finished without result\n";
    let results = parse_batch(text, 2);
    assert_eq!(results[0].exit, Some(0));
    assert_eq!(results[0].result, Some(b"passed".to_vec()));
    assert!(results[0].missing.is_none());
    assert_eq!(results[1].exit, Some(134));
    assert!(results[1].result.is_none());
    assert_eq!(
        results[1].missing.as_deref(),
        Some("run finished without result")
    );
}

#[test]
fn the_batch_parser_keeps_a_missing_case_empty() {
    let results = parse_batch("boot\n", 1);
    assert_eq!(results.len(), 1);
    assert!(results[0].exit.is_none());
    assert!(results[0].result.is_none());
    assert!(results[0].missing.is_none());
}

#[test]
fn exec_read_adds_the_result_reader_service() {
    let (service, reader, svcd) = guest_service(GuestMode::ExecRead);
    assert!(service.contains("args = [\"g1wrap\", \"run\"]"));
    let reader = reader.expect("reader service");
    assert!(reader.contains("args = [\"g1wrap\", \"read\"]"));
    assert!(reader.contains("needs = [\"g1\"]"));
    assert_eq!(svcd, "exit_when_exits = \"g1-read\"\n");
    let (_, reader, svcd) = guest_service(GuestMode::Exec);
    assert!(reader.is_none());
    assert_eq!(svcd, "exit_when_exits = \"g1\"\n");
}

#[test]
fn guest_judging_uses_the_wrapper_exit_code() {
    let pass = GuestObservation {
        exit: Some(0),
        output: Vec::new(),
        result: None,
        result_error: None,
        qemu_status: Some(33),
        timed_out: false,
        serial: String::new(),
    };
    assert_eq!(judge_guest(&pass, 0, 60).verdict, Verdict::Pass);
    let fail = GuestObservation {
        exit: Some(2),
        ..pass.clone()
    };
    assert_eq!(judge_guest(&fail, 0, 60).verdict, Verdict::Fail);
    assert_eq!(
        judge_guest(&fail, 0, 60).reason,
        "behaviour:expected exit 0, got 2"
    );
    let crashed = GuestObservation { exit: None, ..pass };
    assert_eq!(judge_guest(&crashed, 0, 60).verdict, Verdict::Fail);
}

#[test]
fn the_target_environment_has_the_host_runner_values() {
    let env = target_env();
    assert!(env.iter().any(|item| item == "TMPDIR=/tmp"));
    assert!(env.iter().any(|item| item == "TZ=UTC"));
    assert!(env.iter().any(|item| item == "LC_ALL=C.UTF-8"));
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
