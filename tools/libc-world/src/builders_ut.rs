// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::path::Path;

use super::*;
use crate::testutil::TempDir;

fn package(kind: &str) -> Package {
    Package {
        n: 25,
        name: "zlib".to_string(),
        tier: 1,
        build: kind.to_string(),
        version: "PLACEHOLDER".to_string(),
        url: "PLACEHOLDER".to_string(),
        sha256: "PLACEHOLDER".to_string(),
        tarball: "PLACEHOLDER".to_string(),
        needs: Vec::new(),
        optional: false,
        stresses: "off_t width".to_string(),
        configure: vec!["--static".to_string()],
        flag_notes: std::collections::BTreeMap::new(),
        make_args: Vec::new(),
        jobs_make: 4,
        tests: "none".to_string(),
        install: false,
        config_target: "defconfig".to_string(),
        config_lines: Vec::new(),
    }
}

fn context() -> BuildContext {
    BuildContext {
        arch: "x86_64".to_string(),
        triple: "x86_64-unknown-ravenbsd".to_string(),
        build_triple: "x86_64-pc-linux-gnu".to_string(),
        wrapper: "/w/bin/raven-cc".into(),
        ar: "llvm-ar".to_string(),
        ranlib: "llvm-ranlib".to_string(),
        make: "make".to_string(),
        host_cc: "cc".to_string(),
        sysroot: "/w/sysroot".into(),
        toolchain_cmake: "/w/toolchain.cmake".into(),
        host_alias: HostAlias::Raven,
    }
}

fn plan_for(kind: &str) -> Vec<PlannedStep> {
    let package = package(kind);
    plan(
        kind,
        &package,
        &context(),
        Path::new("/w/pkg/src"),
        Path::new("/w/pkg/build"),
        Path::new("/w/pkg/dest"),
    )
    .expect("plan")
}

#[test]
fn host_spec_uses_the_raven_triple() {
    assert_eq!(
        host_spec("x86_64", HostAlias::Raven),
        "x86_64-unknown-ravenbsd"
    );
    assert_eq!(
        host_spec("aarch64", HostAlias::Linux),
        "aarch64-unknown-linux-gnu"
    );
}

#[test]
fn host_alias_parses_both_values() {
    assert_eq!(HostAlias::parse("raven"), Some(HostAlias::Raven));
    assert_eq!(HostAlias::parse("linux"), Some(HostAlias::Linux));
    assert_eq!(HostAlias::parse("bsd"), None);
    assert_eq!(HostAlias::Linux.as_str(), "linux");
}

#[test]
fn autotools_configure_has_the_fixed_flags() {
    let steps = plan_for("ac");
    let configure = &steps[0];
    assert_eq!(configure.name, "configure");
    assert_eq!(configure.program, "/w/pkg/src/configure");
    for flag in [
        "--build=x86_64-pc-linux-gnu",
        "--host=x86_64-unknown-ravenbsd",
        "--prefix=/usr",
        "--sysconfdir=/etc",
        "--localstatedir=/var",
        "--disable-dependency-tracking",
        "--static",
    ] {
        assert!(configure.args.iter().any(|arg| arg == flag), "{flag}");
    }
}

#[test]
fn autotools_make_uses_jobs_make() {
    let steps = plan_for("ac");
    assert_eq!(steps[1].program, "make");
    assert_eq!(steps[1].args, vec!["-j4"]);
}

#[test]
fn autotools_install_passes_destdir() {
    let steps = plan_for("ac");
    let install = steps
        .iter()
        .find(|step| step.name == "install")
        .expect("install");
    assert_eq!(install.args, vec!["install", "DESTDIR=/w/pkg/dest"]);
}

#[test]
fn a_linux_host_alias_changes_only_the_configure_host() {
    let package = package("ac");
    let mut ctx = context();
    ctx.host_alias = HostAlias::Linux;
    let steps = plan(
        "ac",
        &package,
        &ctx,
        Path::new("/w/src"),
        Path::new("/w/build"),
        Path::new("/w/dest"),
    )
    .expect("plan");
    assert!(
        steps[0]
            .args
            .iter()
            .any(|arg| arg == "--host=x86_64-unknown-linux-gnu")
    );
    assert!(
        !steps[0]
            .args
            .iter()
            .any(|arg| arg == "--host=x86_64-unknown-ravenbsd")
    );
}

#[test]
fn custom_configure_sets_the_wrapper() {
    let steps = plan_for("cf");
    assert_eq!(steps[0].name, "configure");
    assert_eq!(steps[0].args[0], "--prefix=/usr");
    assert!(
        steps[0]
            .env
            .iter()
            .any(|(key, value)| key == "CC" && value == "/w/bin/raven-cc")
    );
    assert!(steps[0].env.iter().any(|(key, _)| key == "AR"));
}

#[test]
fn custom_configure_has_no_install_by_default() {
    let steps = plan_for("cf");
    assert!(!steps.iter().any(|step| step.name == "install"));
}

#[test]
fn custom_configure_installs_when_asked() {
    let mut package = package("cf");
    package.install = true;
    let steps = plan(
        "cf",
        &package,
        &context(),
        Path::new("/w/src"),
        Path::new("/w/build"),
        Path::new("/w/dest"),
    )
    .expect("plan");
    let install = steps
        .iter()
        .find(|step| step.name == "install")
        .expect("install");
    assert!(install.args.contains(&"DESTDIR=/w/dest".to_string()));
    assert!(install.args.contains(&"PREFIX=/usr".to_string()));
}

#[test]
fn plain_make_passes_the_toolchain_on_the_command_line() {
    let steps = plan_for("mk");
    let make = &steps[0];
    assert_eq!(make.program, "make");
    for arg in [
        "-j4",
        "CC=/w/bin/raven-cc",
        "AR=llvm-ar",
        "RANLIB=llvm-ranlib",
    ] {
        assert!(make.args.iter().any(|value| value == arg), "{arg}");
    }
}

#[test]
fn plain_make_install_is_optional() {
    let mut package = package("mk");
    package.install = true;
    let steps = plan(
        "mk",
        &package,
        &context(),
        Path::new("/w/src"),
        Path::new("/w/build"),
        Path::new("/w/dest"),
    )
    .expect("plan");
    assert!(steps.iter().any(|step| step.name == "install"));
}

#[test]
fn cmake_configure_uses_the_generated_toolchain() {
    let steps = plan_for("cm");
    let configure = &steps[0];
    assert_eq!(configure.program, "cmake");
    assert!(configure.args.contains(&"-G".to_string()));
    assert!(configure.args.contains(&"Ninja".to_string()));
    assert!(
        configure
            .args
            .contains(&"-DCMAKE_TOOLCHAIN_FILE=/w/toolchain.cmake".to_string())
    );
    assert!(
        configure
            .args
            .contains(&"-DCMAKE_INSTALL_PREFIX=/usr".to_string())
    );
    assert!(
        configure
            .args
            .contains(&"-DCMAKE_BUILD_TYPE=Release".to_string())
    );
}

#[test]
fn cmake_build_and_install_use_the_build_directory() {
    let steps = plan_for("cm");
    assert_eq!(steps[1].args, vec!["--build", "/w/pkg/build"]);
    assert_eq!(steps[2].args, vec!["--install", "/w/pkg/build"]);
    assert!(
        steps[2]
            .env
            .iter()
            .any(|(key, value)| key == "DESTDIR" && value == "/w/pkg/dest")
    );
}

#[test]
fn kconfig_runs_defconfig_then_make_with_hostcc() {
    let steps = plan_for("kc");
    assert_eq!(steps[0].args, vec!["defconfig"]);
    let make = steps.last().expect("make");
    assert_eq!(make.name, "make");
    assert!(make.args.contains(&"CC=/w/bin/raven-cc".to_string()));
    assert!(make.args.contains(&"HOSTCC=cc".to_string()));
}

#[test]
fn kconfig_allnoconfig_and_config_lines_add_oldconfig() {
    let mut package = package("kc");
    package.config_target = "allnoconfig".to_string();
    package.config_lines = vec!["CONFIG_X=y".to_string()];
    let steps = plan(
        "kc",
        &package,
        &context(),
        Path::new("/w/src"),
        Path::new("/w/build"),
        Path::new("/w/dest"),
    )
    .expect("plan");
    assert_eq!(steps[0].args, vec!["allnoconfig"]);
    assert!(steps.iter().any(|step| step.name == "oldconfig"));
}

#[test]
fn an_unimplemented_builder_is_named() {
    let package = package("me");
    let error = plan(
        "me",
        &package,
        &context(),
        Path::new("/w/src"),
        Path::new("/w/build"),
        Path::new("/w/dest"),
    )
    .expect_err("builder");
    assert!(error.to_string().contains("builder-not-implemented:me"));
}

#[test]
fn config_lines_are_appended_to_dot_config() {
    let temp = TempDir::new("builders-config");
    let src = temp.join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(src.join(".config"), "CONFIG_A=y\n").expect("config");
    apply_config_lines(&src, &["CONFIG_B=y".to_string()]).expect("append");
    let text = std::fs::read_to_string(src.join(".config")).expect("read");
    assert_eq!(text, "CONFIG_A=y\nCONFIG_B=y\n");
}

#[test]
fn empty_config_lines_do_not_create_a_file() {
    let temp = TempDir::new("builders-config-empty");
    let src = temp.join("src");
    std::fs::create_dir_all(&src).expect("src");
    apply_config_lines(&src, &[]).expect("none");
    assert!(!src.join(".config").exists());
}
