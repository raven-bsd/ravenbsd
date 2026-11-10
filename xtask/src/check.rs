// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The local gate: `cargo xtask check [--fast]`.
//!
//! Steps run in order and stop at the first failure; every step is reported with its exit code and
//! duration, and the ones after a failure are shown as skipped. `--fast` skips the per-target clippy
//! passes (the slow part) but keeps the host gate and provenance.

use std::time::Instant;

use anyhow::{Context, Result};

use crate::build::{self, Arch, Component};
use crate::docs;
use crate::guard;
use crate::process::{CommandSpec, run as run_command};
use crate::repo::Workspace;

struct Step {
    name: String,
    kind: StepKind,
}

enum StepKind {
    Guard,
    Command(CommandSpec),
    Docs,
    Skipped(String),
}

enum Status {
    Ok,
    Failed(i32),
    Skipped(String),
}

struct Summary {
    name: String,
    status: Status,
    seconds: f64,
}

/// Run the check gate. Returns 0 when every executed step passed, 1 otherwise.
pub(crate) fn run(workspace: &Workspace, fast: bool) -> Result<u8> {
    let steps = build_steps(workspace, fast)?;
    let mut summaries = Vec::new();
    let mut failed = false;
    for step in steps {
        if failed {
            summaries.push(Summary {
                name: step.name,
                status: Status::Skipped("previous failure".to_string()),
                seconds: 0.0,
            });
            continue;
        }
        let started = Instant::now();
        match step.kind {
            StepKind::Skipped(reason) => {
                println!("==> {}: skipped ({reason})", step.name);
                summaries.push(Summary {
                    name: step.name,
                    status: Status::Skipped(reason),
                    seconds: 0.0,
                });
            }
            StepKind::Guard => {
                println!("==> {}", step.name);
                let violations = guard::scan_repo(&workspace.root)?;
                guard::print_violations(&violations);
                let code = i32::from(!violations.is_empty());
                let seconds = started.elapsed().as_secs_f64();
                if code != 0 {
                    failed = true;
                }
                summaries.push(Summary {
                    name: step.name,
                    status: status_for(code),
                    seconds,
                });
            }
            StepKind::Command(spec) => {
                println!("==> {}", step.name);
                let code = run_command(&spec)
                    .with_context(|| format!("cannot run check step `{}`", step.name))?;
                let seconds = started.elapsed().as_secs_f64();
                if code != 0 {
                    failed = true;
                }
                summaries.push(Summary {
                    name: step.name,
                    status: status_for(code),
                    seconds,
                });
            }
            StepKind::Docs => {
                println!("==> {}", step.name);
                let code = i32::from(docs::run(&workspace.root)?);
                let seconds = started.elapsed().as_secs_f64();
                if code != 0 {
                    failed = true;
                }
                summaries.push(Summary {
                    name: step.name,
                    status: status_for(code),
                    seconds,
                });
            }
        }
    }
    print_summary(&summaries);
    Ok(u8::from(failed))
}

fn status_for(code: i32) -> Status {
    if code == 0 {
        Status::Ok
    } else {
        Status::Failed(code)
    }
}

fn print_summary(summaries: &[Summary]) {
    println!("check summary:");
    for summary in summaries {
        match &summary.status {
            Status::Ok => println!(
                "  ok       {} (exit 0, {:.2}s)",
                summary.name, summary.seconds
            ),
            Status::Failed(code) => println!(
                "  FAILED   {} (exit {code}, {:.2}s)",
                summary.name, summary.seconds
            ),
            Status::Skipped(reason) => println!("  skipped  {} ({reason})", summary.name),
        }
    }
    let failed = summaries
        .iter()
        .any(|summary| matches!(summary.status, Status::Failed(_)));
    if failed {
        println!("check: FAILED");
    } else {
        println!("check: PASSED");
    }
}

fn build_steps(workspace: &Workspace, fast: bool) -> Result<Vec<Step>> {
    let root = &workspace.root;
    let mut steps = vec![
        Step {
            name: "guard".to_string(),
            kind: StepKind::Guard,
        },
        command_step(
            "fmt",
            root,
            "cargo",
            vec!["fmt".into(), "--all".into(), "--".into(), "--check".into()],
        ),
        command_step("clippy (host)", root, "cargo", host_clippy_args(workspace)),
        command_step("tests (host)", root, "cargo", vec!["test".into()]),
    ];
    if root.join("crates/raven-provenance/Cargo.toml").is_file() {
        steps.push(command_step(
            "provenance",
            root,
            "cargo",
            vec![
                "run".into(),
                "-q".into(),
                "-p".into(),
                "raven-provenance".into(),
                "--".into(),
                "check".into(),
                "--root".into(),
                ".".into(),
            ],
        ));
    } else {
        steps.push(Step {
            name: "provenance".to_string(),
            kind: StepKind::Skipped("B02 not merged".to_string()),
        });
    }

    let mut target_steps = Vec::new();
    if let Some(kernel) = workspace.kernel() {
        target_steps.push(target_clippy_step(
            workspace,
            "clippy kernel x86_64",
            Component::Kernel,
            Arch::X86_64,
            std::slice::from_ref(&kernel.name),
        )?);
        target_steps.push(target_clippy_step(
            workspace,
            "clippy kernel aarch64",
            Component::Kernel,
            Arch::Aarch64,
            std::slice::from_ref(&kernel.name),
        )?);
    }
    let user = workspace.user_packages();
    if user.is_empty() {
        target_steps.push(Step {
            name: "clippy user programs x86_64".to_string(),
            kind: StepKind::Skipped("no user crates yet".to_string()),
        });
    } else {
        let names: Vec<String> = user.iter().map(|package| package.name.clone()).collect();
        target_steps.push(target_clippy_step(
            workspace,
            "clippy user programs x86_64",
            Component::User,
            Arch::X86_64,
            &names,
        )?);
    }
    if let Some(loader) = workspace.loader() {
        target_steps.push(target_clippy_step(
            workspace,
            "clippy loader x86_64",
            Component::Loader,
            Arch::X86_64,
            std::slice::from_ref(&loader.name),
        )?);
    }
    if fast {
        for step in target_steps {
            steps.push(Step {
                name: step.name,
                kind: StepKind::Skipped("--fast".to_string()),
            });
        }
    } else {
        steps.extend(target_steps);
    }
    steps.push(Step {
        name: "docs".to_string(),
        kind: StepKind::Docs,
    });
    Ok(steps)
}

/// Arguments for host clippy: workspace-wide minus every target-only crate.
pub(crate) fn host_clippy_args(workspace: &Workspace) -> Vec<String> {
    let mut args = vec!["clippy".to_string(), "--workspace".to_string()];
    for name in workspace.target_only_names() {
        args.push("--exclude".to_string());
        args.push(name);
    }
    args.extend([
        "--all-targets".to_string(),
        "--".to_string(),
        "-D".to_string(),
        "warnings".to_string(),
    ]);
    args
}

fn target_clippy_step(
    workspace: &Workspace,
    name: &str,
    component: Component,
    arch: Arch,
    packages: &[String],
) -> Result<Step> {
    let target = build::target_for(component, arch)?;
    let target_dir = workspace.root.join("build/cargo").join(format!(
        "{}-{}",
        component.as_str(),
        arch.as_str()
    ));
    let linker = workspace
        .root
        .join("userland/linker")
        .join(format!("{}.ld", arch.as_str()));
    let linker = linker.is_file().then_some(linker);
    let flags = build::rustflags_string(component, arch, &workspace.root, linker.as_deref());
    let mut args = vec!["clippy".to_string()];
    for package in packages {
        args.push("-p".to_string());
        args.push(package.clone());
    }
    args.extend([
        "--target".to_string(),
        target.to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
        "--".to_string(),
        "-D".to_string(),
        "warnings".to_string(),
    ]);
    Ok(Step {
        name: name.to_string(),
        kind: StepKind::Command(
            CommandSpec::new("cargo", args, &workspace.root)
                .env("RUSTFLAGS", flags)
                .env_remove("CARGO_ENCODED_RUSTFLAGS"),
        ),
    })
}

fn command_step(name: &str, root: &std::path::Path, program: &str, args: Vec<String>) -> Step {
    Step {
        name: name.to_string(),
        kind: StepKind::Command(CommandSpec::new(program, args, root)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{Package, TargetKind};
    use std::path::PathBuf;

    fn workspace() -> Workspace {
        Workspace {
            root: PathBuf::from("/repo"),
            packages: vec![
                Package {
                    name: "raven-kernel".into(),
                    rel_dir: "kernel".into(),
                    target: Some(TargetKind::Bare),
                    component: Some(Component::Kernel),
                },
                Package {
                    name: "raven-elf".into(),
                    rel_dir: "crates/raven-elf".into(),
                    target: None,
                    component: None,
                },
            ],
        }
    }

    #[test]
    fn host_clippy_excludes_target_crates() {
        let args = host_clippy_args(&workspace());
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--exclude", "raven-kernel"])
        );
        assert!(!args.iter().any(|arg| arg == "raven-elf"));
        assert!(args.iter().any(|arg| arg == "-D"));
    }

    #[test]
    fn fast_mode_skips_target_steps() {
        let steps = build_steps(&workspace(), true).unwrap();
        let target_steps: Vec<&Step> = steps
            .iter()
            .filter(|step| {
                step.name.starts_with("clippy kernel")
                    || step.name.starts_with("clippy user")
                    || step.name.starts_with("clippy loader")
            })
            .collect();
        assert!(!target_steps.is_empty());
        assert!(
            target_steps
                .iter()
                .all(|step| matches!(step.kind, StepKind::Skipped(_)))
        );
    }

    #[test]
    fn normal_mode_runs_kernel_clippy() {
        let steps = build_steps(&workspace(), false).unwrap();
        let kernel = steps
            .iter()
            .find(|step| step.name == "clippy kernel x86_64")
            .unwrap();
        assert!(matches!(kernel.kind, StepKind::Command(_)));
    }

    #[test]
    fn normal_mode_runs_aarch64_kernel_clippy() {
        let steps = build_steps(&workspace(), false).unwrap();
        let kernel = steps
            .iter()
            .find(|step| step.name == "clippy kernel aarch64")
            .unwrap();
        let StepKind::Command(spec) = &kernel.kind else {
            panic!("aarch64 kernel clippy step is not a command");
        };
        assert!(
            spec.args
                .windows(2)
                .any(|pair| pair == ["--target", "aarch64-unknown-none-softfloat"])
        );
        assert!(spec.args.iter().any(|arg| arg == "raven-kernel"));
    }
}
