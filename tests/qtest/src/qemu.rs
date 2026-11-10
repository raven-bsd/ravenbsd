// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Firmware discovery and QEMU/image command construction (pure functions).

use std::env;
use std::path::{Path, PathBuf};

const OVMF_CODE: &str = "/usr/share/OVMF/OVMF_CODE_4M.fd";
const OVMF_VARS: &str = "/usr/share/OVMF/OVMF_VARS_4M.fd";
const AAVMF_CODE: &str = "/usr/share/AAVMF/AAVMF_CODE.fd";
const AAVMF_VARS: &str = "/usr/share/AAVMF/AAVMF_VARS.fd";

/// Paired pflash images for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Firmware {
    pub code: PathBuf,
    pub vars: PathBuf,
}

/// A program and its argument vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QemuCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// Find firmware paths: CLI first, then environment, then the x86-64 defaults.
pub(crate) fn resolve_firmware(
    arch: &str,
    cli_code: Option<&Path>,
    cli_vars: Option<&Path>,
) -> Result<Firmware, String> {
    let (env_code_name, env_vars_name) = match arch {
        "aarch64" => ("RAVEN_AAVMF_CODE", "RAVEN_AAVMF_VARS"),
        _ => ("RAVEN_OVMF_CODE", "RAVEN_OVMF_VARS"),
    };
    let env_code = env::var_os(env_code_name).map(PathBuf::from);
    let env_vars = env::var_os(env_vars_name).map(PathBuf::from);
    select_firmware(
        arch,
        cli_code,
        cli_vars,
        env_code.as_deref(),
        env_vars.as_deref(),
    )
}

/// Default pflash images for an architecture, when the CLI and environment are silent.
pub(crate) fn default_firmware(arch: &str) -> Option<(&'static str, &'static str)> {
    match arch {
        "x86_64" => Some((OVMF_CODE, OVMF_VARS)),
        "aarch64" => Some((AAVMF_CODE, AAVMF_VARS)),
        _ => None,
    }
}

/// Pure firmware selection, separated from the environment for tests.
pub(crate) fn select_firmware(
    arch: &str,
    cli_code: Option<&Path>,
    cli_vars: Option<&Path>,
    env_code: Option<&Path>,
    env_vars: Option<&Path>,
) -> Result<Firmware, String> {
    let (default_code, default_vars) = default_firmware(arch)
        .ok_or_else(|| format!("no firmware discovery rule for arch `{arch}`"))?;
    let (code, vars) = (
        cli_code
            .or(env_code)
            .map_or_else(|| PathBuf::from(default_code), Path::to_path_buf),
        cli_vars
            .or(env_vars)
            .map_or_else(|| PathBuf::from(default_vars), Path::to_path_buf),
    );
    if !code.is_file() {
        return Err(format!("firmware code not found: {}", code.display()));
    }
    if !vars.is_file() {
        return Err(format!("firmware vars not found: {}", vars.display()));
    }
    Ok(Firmware { code, vars })
}

/// QEMU program name for an architecture.
pub(crate) fn program_for(arch: &str) -> Result<String, String> {
    match arch {
        "x86_64" => Ok("qemu-system-x86_64".to_string()),
        "aarch64" => Ok("qemu-system-aarch64".to_string()),
        "riscv64" => Ok("qemu-system-riscv64".to_string()),
        other => Err(format!("unknown architecture `{other}`")),
    }
}

/// Build the fixed QEMU arguments from `architecture/02` section 6 plus the case extras.
pub(crate) fn qemu_command(
    arch: &str,
    image: &Path,
    firmware: &Firmware,
    extra: &[String],
) -> Result<QemuCommand, String> {
    let program = program_for(arch)?;
    let mut args: Vec<String> = Vec::new();
    match arch {
        "x86_64" => {
            for argument in [
                "-machine",
                "q35",
                "-m",
                "512M",
                "-smp",
                "4",
                "-serial",
                "stdio",
                "-display",
                "none",
                "-monitor",
                "none",
                "-no-reboot",
            ] {
                args.push(argument.to_string());
            }
            args.push("-drive".to_string());
            args.push(format!(
                "if=pflash,format=raw,readonly=on,file={}",
                firmware.code.display()
            ));
            args.push("-drive".to_string());
            args.push(format!(
                "if=pflash,format=raw,file={}",
                firmware.vars.display()
            ));
            args.push("-drive".to_string());
            args.push(format!("format=raw,file={}", image.display()));
            args.push("-device".to_string());
            args.push("isa-debug-exit,iobase=0xf4,iosize=0x04".to_string());
        }
        "aarch64" => {
            for argument in [
                "-M",
                "virt",
                "-cpu",
                "cortex-a72",
                "-m",
                "512M",
                "-nographic",
                "-semihosting-config",
                "enable=on,target=native",
            ] {
                args.push(argument.to_string());
            }
            args.push("-drive".to_string());
            args.push(format!(
                "if=pflash,format=raw,readonly=on,file={}",
                firmware.code.display()
            ));
            args.push("-drive".to_string());
            args.push(format!(
                "if=pflash,format=raw,file={}",
                firmware.vars.display()
            ));
            args.push("-drive".to_string());
            args.push(format!("format=raw,file={}", image.display()));
        }
        other => return Err(format!("no QEMU command line for arch `{other}`")),
    }
    args.extend(extra.iter().cloned());
    Ok(QemuCommand { program, args })
}

/// Command that asks xtask (K05) to build a per-case image.
pub(crate) fn image_command(
    arch: &str,
    profile: &str,
    boot_cmdline: Option<&str>,
    output: &Path,
) -> (String, Vec<String>) {
    let mut args = vec![
        "xtask".to_string(),
        "image".to_string(),
        "--arch".to_string(),
        arch.to_string(),
        "--profile".to_string(),
        profile.to_string(),
    ];
    if let Some(cmdline) = boot_cmdline {
        args.push("--cmdline".to_string());
        args.push(cmdline.to_string());
    }
    args.push("--output".to_string());
    args.push(output.display().to_string());
    ("cargo".to_string(), args)
}

/// Whether a program is present in `PATH`.
pub(crate) fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|directory| directory.join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn firmware(temp: &TempDir) -> Firmware {
        let code = temp.path().join("CODE.fd");
        let vars = temp.path().join("VARS.fd");
        std::fs::write(&code, b"code").unwrap();
        std::fs::write(&vars, b"vars").unwrap();
        Firmware { code, vars }
    }

    #[test]
    fn x86_command_matches_the_contract() {
        let temp = TempDir::new("qemu-x86");
        let firmware = firmware(&temp);
        let command =
            qemu_command("x86_64", Path::new("build/qtest/case.img"), &firmware, &[]).unwrap();
        assert_eq!(command.program, "qemu-system-x86_64");
        assert_eq!(
            command.args,
            vec![
                "-machine",
                "q35",
                "-m",
                "512M",
                "-smp",
                "4",
                "-serial",
                "stdio",
                "-display",
                "none",
                "-monitor",
                "none",
                "-no-reboot",
                "-drive",
                "if=pflash,format=raw,readonly=on,file=<CODE>",
                "-drive",
                "if=pflash,format=raw,file=<VARS>",
                "-drive",
                "format=raw,file=build/qtest/case.img",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
            ]
            .into_iter()
            .map(|value| {
                value
                    .replace("<CODE>", &firmware.code.display().to_string())
                    .replace("<VARS>", &firmware.vars.display().to_string())
            })
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn aarch64_command_matches_the_contract() {
        let temp = TempDir::new("qemu-arm");
        let firmware = firmware(&temp);
        let command =
            qemu_command("aarch64", Path::new("build/qtest/case.img"), &firmware, &[]).unwrap();
        assert_eq!(command.program, "qemu-system-aarch64");
        assert_eq!(
            command.args,
            vec![
                "-M",
                "virt",
                "-cpu",
                "cortex-a72",
                "-m",
                "512M",
                "-nographic",
                "-semihosting-config",
                "enable=on,target=native",
                "-drive",
                "if=pflash,format=raw,readonly=on,file=<CODE>",
                "-drive",
                "if=pflash,format=raw,file=<VARS>",
                "-drive",
                "format=raw,file=build/qtest/case.img",
            ]
            .into_iter()
            .map(|value| {
                value
                    .replace("<CODE>", &firmware.code.display().to_string())
                    .replace("<VARS>", &firmware.vars.display().to_string())
            })
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn qemu_extra_is_appended_last() {
        let temp = TempDir::new("qemu-extra");
        let firmware = firmware(&temp);
        let extra = vec!["-d".to_string(), "guest_errors".to_string()];
        let command = qemu_command(
            "x86_64",
            Path::new("build/qtest/case.img"),
            &firmware,
            &extra,
        )
        .unwrap();
        assert_eq!(&command.args[command.args.len() - 2..], extra.as_slice());
    }

    #[test]
    fn cli_firmware_wins_over_environment() {
        let temp = TempDir::new("firmware-cli");
        let cli_code = temp.path().join("cli-code.fd");
        let cli_vars = temp.path().join("cli-vars.fd");
        let env_code = temp.path().join("env-code.fd");
        let env_vars = temp.path().join("env-vars.fd");
        for path in [&cli_code, &cli_vars, &env_code, &env_vars] {
            std::fs::write(path, b"x").unwrap();
        }
        let selected = select_firmware(
            "x86_64",
            Some(&cli_code),
            Some(&cli_vars),
            Some(&env_code),
            Some(&env_vars),
        )
        .unwrap();
        assert_eq!(selected.code, cli_code);
        assert_eq!(selected.vars, cli_vars);
    }

    #[test]
    fn environment_wins_over_x86_defaults() {
        let temp = TempDir::new("firmware-env");
        let env_code = temp.path().join("env-code.fd");
        let env_vars = temp.path().join("env-vars.fd");
        std::fs::write(&env_code, b"x").unwrap();
        std::fs::write(&env_vars, b"x").unwrap();
        let selected =
            select_firmware("x86_64", None, None, Some(&env_code), Some(&env_vars)).unwrap();
        assert_eq!(selected.code, env_code);
        assert_eq!(selected.vars, env_vars);
    }

    #[test]
    fn missing_firmware_is_a_reason() {
        let error = select_firmware(
            "x86_64",
            Some(Path::new("/no/such/code.fd")),
            Some(Path::new("/no/such/vars.fd")),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.contains("firmware code not found"), "{error}");
    }

    #[test]
    fn aarch64_missing_firmware_is_a_reason() {
        let error = select_firmware(
            "aarch64",
            Some(Path::new("/no/such/code.fd")),
            Some(Path::new("/no/such/vars.fd")),
            None,
            None,
        )
        .unwrap_err();
        assert!(error.contains("firmware code not found"), "{error}");
    }

    #[test]
    fn aarch64_defaults_are_aavmf() {
        let (code, vars) = default_firmware("aarch64").unwrap();
        assert_eq!(code, "/usr/share/AAVMF/AAVMF_CODE.fd");
        assert_eq!(vars, "/usr/share/AAVMF/AAVMF_VARS.fd");
        let (code, vars) = default_firmware("x86_64").unwrap();
        assert_eq!(code, OVMF_CODE);
        assert_eq!(vars, OVMF_VARS);
    }

    #[test]
    fn unsupported_arch_has_a_reason() {
        let error = select_firmware("sparc64", None, None, None, None).unwrap_err();
        assert!(error.contains("sparc64"), "{error}");
    }

    #[test]
    fn image_command_matches_the_contract() {
        let (program, args) = image_command(
            "x86_64",
            "base",
            Some("console=ttyS0 test-exit=1"),
            Path::new("build/qtest/boot.img"),
        );
        assert_eq!(program, "cargo");
        assert_eq!(
            args,
            vec![
                "xtask",
                "image",
                "--arch",
                "x86_64",
                "--profile",
                "base",
                "--cmdline",
                "console=ttyS0 test-exit=1",
                "--output",
                "build/qtest/boot.img",
            ]
        );
    }

    #[test]
    fn image_command_without_cmdline() {
        let (_, args) = image_command("aarch64", "base", None, Path::new("out.img"));
        assert!(!args.contains(&"--cmdline".to_string()));
        assert_eq!(args[args.len() - 2], "--output");
    }

    #[test]
    fn program_names_are_arch_specific() {
        assert_eq!(program_for("x86_64").unwrap(), "qemu-system-x86_64");
        assert_eq!(program_for("aarch64").unwrap(), "qemu-system-aarch64");
        assert_eq!(program_for("riscv64").unwrap(), "qemu-system-riscv64");
        assert!(program_for("mips").is_err());
    }
}
