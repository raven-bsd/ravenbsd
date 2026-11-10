// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! QEMU launch: firmware selection, a stable argument vector and exit-code mapping.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::build::Arch;

const OVMF_CODE: &str = "/usr/share/OVMF/OVMF_CODE_4M.fd";
const OVMF_VARS: &str = "/usr/share/OVMF/OVMF_VARS_4M.fd";
const AAVMF_CODE: &str = "/usr/share/AAVMF/AAVMF_CODE.fd";
const AAVMF_VARS: &str = "/usr/share/AAVMF/AAVMF_VARS.fd";

/// Build (unless disabled) and run the image under QEMU.
pub(crate) fn run(
    root: &Path,
    arch: Arch,
    profile: &str,
    no_build: bool,
    extra: &[String],
) -> Result<u8> {
    if arch == Arch::Riscv64 {
        bail!("run is not supported for riscv64 yet");
    }
    if !no_build {
        let code = crate::image::run(root, arch, profile, None, None)?;
        if code != 0 {
            bail!("image build failed with exit code {code}");
        }
    }

    let image = root
        .join("build")
        .join(arch.as_str())
        .join(format!("ravenbsd-{}.img", arch.as_str()));
    if !image.is_file() {
        bail!(
            "image not found: {} (run without --no-build first)",
            image.display()
        );
    }

    let (code, vars) = firmware(arch)?;
    let copied_vars = copy_vars(root, arch, &vars)?;
    let mut extra = extra.to_vec();
    let kvm = if let Some(index) = extra.iter().position(|argument| argument == "--kvm") {
        extra.remove(index);
        true
    } else {
        false
    };
    let mut command = Command::new(program(arch));
    let args = qemu_args(
        arch,
        &image,
        &code,
        &copied_vars,
        kvm,
        profile == "shell",
        &extra,
    );
    command.args(&args);
    println!("==> {} {}", program(arch), args.join(" "));
    let status = command
        .status()
        .with_context(|| format!("cannot run `{}`", program(arch)))?;
    Ok(map_exit(status.code()))
}

fn program(arch: Arch) -> &'static str {
    match arch {
        Arch::X86_64 => "qemu-system-x86_64",
        Arch::Aarch64 => "qemu-system-aarch64",
        Arch::Riscv64 => "qemu-system-riscv64",
    }
}

fn firmware(arch: Arch) -> Result<(PathBuf, PathBuf)> {
    let (code, vars) = match arch {
        Arch::X86_64 => (
            std::env::var("RAVEN_OVMF_CODE").unwrap_or_else(|_| OVMF_CODE.to_string()),
            std::env::var("RAVEN_OVMF_VARS").unwrap_or_else(|_| OVMF_VARS.to_string()),
        ),
        Arch::Aarch64 => (
            std::env::var("RAVEN_AAVMF_CODE").unwrap_or_else(|_| AAVMF_CODE.to_string()),
            std::env::var("RAVEN_AAVMF_VARS").unwrap_or_else(|_| AAVMF_VARS.to_string()),
        ),
        Arch::Riscv64 => bail!("run is not supported for riscv64 yet"),
    };
    let code = PathBuf::from(code);
    let vars = PathBuf::from(vars);
    if !code.is_file() {
        bail!("QEMU firmware code not found: {}", code.display());
    }
    if !vars.is_file() {
        bail!("QEMU firmware vars not found: {}", vars.display());
    }
    Ok((code, vars))
}

fn copy_vars(root: &Path, arch: Arch, source: &Path) -> Result<PathBuf> {
    let directory = root.join("build").join(arch.as_str());
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let name = match arch {
        Arch::X86_64 => "OVMF_VARS.fd",
        Arch::Aarch64 => "AAVMF_VARS.fd",
        Arch::Riscv64 => bail!("run is not supported for riscv64 yet"),
    };
    let destination = directory.join(name);
    fs::copy(source, &destination).with_context(|| {
        format!(
            "cannot copy QEMU vars {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

fn qemu_args(
    arch: Arch,
    image: &Path,
    code: &Path,
    vars: &Path,
    kvm: bool,
    interactive: bool,
    extra: &[String],
) -> Vec<String> {
    let mut args = match arch {
        Arch::X86_64 => vec![
            "-machine".to_string(),
            "q35".to_string(),
            "-m".to_string(),
            "512M".to_string(),
            "-smp".to_string(),
            "4".to_string(),
            "-serial".to_string(),
            if interactive { "mon:stdio" } else { "stdio" }.to_string(),
            "-display".to_string(),
            "none".to_string(),
            "-no-reboot".to_string(),
            "-drive".to_string(),
            format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
            "-drive".to_string(),
            format!("if=pflash,format=raw,file={}", vars.display()),
            "-drive".to_string(),
            format!("format=raw,file={}", image.display()),
            "-device".to_string(),
            "isa-debug-exit,iobase=0xf4,iosize=0x04".to_string(),
        ],
        Arch::Aarch64 => vec![
            "-M".to_string(),
            "virt".to_string(),
            "-cpu".to_string(),
            "cortex-a72".to_string(),
            "-m".to_string(),
            "512M".to_string(),
            "-smp".to_string(),
            "4".to_string(),
            "-nographic".to_string(),
            "-drive".to_string(),
            format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
            "-drive".to_string(),
            format!("if=pflash,format=raw,file={}", vars.display()),
            "-drive".to_string(),
            format!("format=raw,file={}", image.display()),
        ],
        Arch::Riscv64 => Vec::new(),
    };
    if kvm {
        args.push("-enable-kvm".to_string());
    }
    if arch == Arch::X86_64 && !interactive {
        args.insert(12, "none".to_string());
        args.insert(12, "-monitor".to_string());
    }
    args.extend(extra.iter().cloned());
    args
}

fn map_exit(code: Option<i32>) -> u8 {
    match code {
        Some(33) => 0,
        Some(35) => 1,
        Some(code) => u8::try_from(code).unwrap_or(1),
        None => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_command_matches_architecture_02() {
        let args = qemu_args(
            Arch::X86_64,
            Path::new("/img"),
            Path::new("/code"),
            Path::new("/vars"),
            false,
            false,
            &[],
        );
        assert_eq!(args[0], "-machine");
        assert_eq!(args[1], "q35");
        assert_eq!(args[5], "4");
        assert!(args.iter().any(|arg| arg.contains("isa-debug-exit")));
        assert!(args.iter().any(|arg| arg == "format=raw,file=/img"));
        assert!(!args.iter().any(|arg| arg == "-enable-kvm"));
    }

    #[test]
    fn kvm_is_opt_in_and_extra_args_are_appended() {
        let args = qemu_args(
            Arch::X86_64,
            Path::new("/img"),
            Path::new("/code"),
            Path::new("/vars"),
            true,
            false,
            &["-display".to_string(), "gtk".to_string()],
        );
        assert!(args.iter().any(|arg| arg == "-enable-kvm"));
        assert_eq!(args[args.len() - 2], "-display");
        assert_eq!(args[args.len() - 1], "gtk");
    }

    #[test]
    fn aarch64_command_uses_virt_and_nographic() {
        let args = qemu_args(
            Arch::Aarch64,
            Path::new("/img"),
            Path::new("/code"),
            Path::new("/vars"),
            false,
            false,
            &[],
        );
        assert_eq!(args[0], "-M");
        assert_eq!(args[1], "virt");
        assert_eq!(args[3], "cortex-a72");
        assert!(args.iter().any(|arg| arg == "-nographic"));
        assert!(!args.iter().any(|arg| arg.contains("isa-debug-exit")));
    }

    #[test]
    fn test_exit_codes_map_to_success_and_failure() {
        assert_eq!(map_exit(Some(33)), 0);
        assert_eq!(map_exit(Some(35)), 1);
        assert_eq!(map_exit(Some(0)), 0);
        assert_eq!(map_exit(Some(1)), 1);
        assert_eq!(map_exit(None), 1);
    }

    #[test]
    fn shell_uses_multiplexed_serial_without_a_separate_monitor() {
        let args = qemu_args(
            Arch::X86_64,
            Path::new("/img"),
            Path::new("/code"),
            Path::new("/vars"),
            false,
            true,
            &[],
        );
        assert!(args.windows(2).any(|pair| pair == ["-serial", "mon:stdio"]));
        assert!(!args.windows(2).any(|pair| pair == ["-monitor", "none"]));
    }
}
