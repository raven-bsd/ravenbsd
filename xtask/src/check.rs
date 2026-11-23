// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The local gate: `cargo xtask check [--fast] [--changed]`.
//!
//! Every step runs, even after an earlier step failed, so one invocation reports every problem.
//! A step that cannot be started is reported as failed and the remaining steps still run. `--fast`
//! skips the per-target clippy passes (the slow part) but keeps the host gate and provenance.
//! `--changed` runs only the cargo packages touched by the branch or the working tree: the fast
//! per-edit gate. The full `make check` remains the checkpoint gate.

use std::collections::BTreeSet;
use std::path::Path;
use std::thread;
use std::time::Instant;

use anyhow::Result;

use crate::build::{self, Arch, Component};
use crate::docs;
use crate::guard;
use crate::process::{CommandSpec, run as run_command};
use crate::profiles;
use crate::repo::{Package, Workspace};

struct Step {
    name: String,
    kind: StepKind,
}

enum StepKind {
    Guard,
    Command(CommandSpec),
    Docs,
    Profiles,
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
pub(crate) fn run(workspace: &Workspace, fast: bool, changed: bool) -> Result<u8> {
    let steps = build_steps(workspace, fast, changed)?;
    let order: Vec<String> = steps.iter().map(|step| step.name.clone()).collect();
    // The guard scan and every command step are independent passes; run them beside each other
    // instead of strictly after one another. The tests step dominates the gate, and the cargo
    // steps release the target lock while they run, so fmt/clippy/doc overlap it (K548 rev335 /
    // rev339 gate wall). Every job measures its own wall time; the gate wall is the longest job.
    let mut deferred = Vec::new();
    let mut jobs: Vec<(String, thread::JoinHandle<(Status, f64)>)> = Vec::new();
    for step in steps {
        let Step { name, kind } = step;
        match kind {
            StepKind::Guard => {
                println!("==> {name}");
                let root = workspace.root.clone();
                let job = thread::spawn(move || {
                    let started = Instant::now();
                    let status = match guard::scan_repo(&root) {
                        Ok(violations) => {
                            guard::print_violations(&violations);
                            status_for(i32::from(!violations.is_empty()))
                        }
                        Err(error) => {
                            eprintln!("check: `guard` could not run: {error:#}");
                            Status::Failed(1)
                        }
                    };
                    (status, started.elapsed().as_secs_f64())
                });
                jobs.push((name, job));
            }
            StepKind::Command(spec) => {
                println!("==> {name}");
                let label = name.clone();
                let job = thread::spawn(move || {
                    let started = Instant::now();
                    let status = match run_command(&spec) {
                        Ok(code) => status_for(code),
                        Err(error) => {
                            eprintln!("check: `{label}` could not run: {error:#}");
                            Status::Failed(1)
                        }
                    };
                    (status, started.elapsed().as_secs_f64())
                });
                jobs.push((name, job));
            }
            other => deferred.push(Step { name, kind: other }),
        }
    }
    let mut summaries = Vec::new();
    let mut failed = false;
    for step in deferred {
        let Step { name, kind } = step;
        let started = Instant::now();
        let status = match kind {
            StepKind::Skipped(reason) => {
                println!("==> {name}: skipped ({reason})");
                Status::Skipped(reason)
            }
            StepKind::Guard => Status::Failed(1),
            StepKind::Command(spec) => {
                println!("==> {name}");
                match run_command(&spec) {
                    Ok(code) => status_for(code),
                    Err(error) => {
                        eprintln!("check: `{name}` could not run: {error:#}");
                        Status::Failed(1)
                    }
                }
            }
            StepKind::Docs => {
                println!("==> {name}");
                match docs::run(&workspace.root) {
                    Ok(code) => status_for(i32::from(code)),
                    Err(error) => {
                        eprintln!("check: `{name}` could not run: {error:#}");
                        Status::Failed(1)
                    }
                }
            }
            StepKind::Profiles => {
                println!("==> {name}");
                match profiles::one_shot_restart_violations(&workspace.root) {
                    Ok(violations) => {
                        for violation in &violations {
                            println!("{violation}");
                        }
                        status_for(i32::from(!violations.is_empty()))
                    }
                    Err(error) => {
                        eprintln!("check: `{name}` could not run: {error:#}");
                        Status::Failed(1)
                    }
                }
            }
        };
        let seconds = started.elapsed().as_secs_f64();
        if matches!(status, Status::Failed(_)) {
            failed = true;
        }
        summaries.push(Summary {
            name,
            status,
            seconds,
        });
    }
    for (name, job) in jobs {
        let (status, seconds) = job.join().unwrap_or((Status::Failed(1), 0.0));
        if matches!(status, Status::Failed(_)) {
            failed = true;
        }
        summaries.push(Summary {
            name,
            status,
            seconds,
        });
    }
    summaries.sort_by_key(|summary| {
        order
            .iter()
            .position(|name| name == &summary.name)
            .unwrap_or(usize::MAX)
    });
    print_summary(&summaries, changed);
    Ok(u8::from(failed))
}

fn status_for(code: i32) -> Status {
    if code == 0 {
        Status::Ok
    } else {
        Status::Failed(code)
    }
}

fn print_summary(summaries: &[Summary], changed: bool) {
    let verdict = if changed { "check-fast" } else { "check" };
    println!("{verdict} summary:");
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
        println!("{verdict}: FAILED");
    } else {
        println!("{verdict}: PASSED");
    }
}

fn build_steps(workspace: &Workspace, fast: bool, changed: bool) -> Result<Vec<Step>> {
    let root = &workspace.root;
    if changed && let Some(packages) = changed_packages(workspace)? {
        return Ok(changed_steps(workspace, &packages));
    }
    let nextest = have_nextest();
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
    ];
    if nextest {
        steps.push(command_step(
            "tests (host, parallel)",
            root,
            "cargo",
            vec![
                "nextest".into(),
                "run".into(),
                "--status-level=fail".into(),
                "--no-fail-fast".into(),
            ],
        ));
        steps.push(command_step(
            "tests (doc)",
            root,
            "cargo",
            vec!["test".into(), "--doc".into()],
        ));
    } else {
        steps.push(command_step(
            "tests (host)",
            root,
            "cargo",
            vec!["test".into()],
        ));
    }
    steps.push(Step {
        name: "profiles".to_string(),
        kind: StepKind::Profiles,
    });
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
                "--strict".into(),
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
    steps.push(command_step(
        "frozen writes",
        &workspace.root,
        "sh",
        vec!["scripts/check-frozen-writes.sh".to_string()],
    ));
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
    let flags = build::rustflags_string(component, arch, &workspace.root, linker.as_deref(), false);
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

/// Whether the host has `cargo-nextest`, the parallel test runner. It schedules test binaries
/// across cores and prints only failures; without it the gate falls back to serial `cargo test`.
fn have_nextest() -> bool {
    std::process::Command::new("cargo")
        .args(["nextest", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Packages touched by the branch or the working tree. `None` means "run the full gate": a
/// sensitive root file changed, git is unavailable, or no diff could be read.
fn changed_packages(workspace: &Workspace) -> Result<Option<Vec<&Package>>> {
    let Some(files) = changed_files(&workspace.root) else {
        return Ok(None);
    };
    if files.is_empty() {
        return Ok(None);
    }
    const SENSITIVE: &[&str] = &[
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "Makefile",
    ];
    if files.iter().any(|file| {
        SENSITIVE.contains(&file.as_str())
            || file.starts_with("xtask/")
            || file.starts_with("scripts/")
    }) {
        return Ok(None);
    }
    let mut names = BTreeSet::new();
    for file in &files {
        let mut best: Option<&Package> = None;
        for package in &workspace.packages {
            let inside = file == &package.rel_dir
                || file
                    .strip_prefix(&package.rel_dir)
                    .is_some_and(|rest| rest.starts_with('/'));
            if inside && best.is_none_or(|current| package.rel_dir.len() > current.rel_dir.len()) {
                best = Some(package);
            }
        }
        if let Some(package) = best {
            names.insert(package.name.as_str());
        }
    }
    Ok(Some(
        workspace
            .packages
            .iter()
            .filter(|package| names.contains(package.name.as_str()))
            .collect(),
    ))
}

/// Committed, unstaged and staged paths, so the fast gate sees work in progress.
fn changed_files(root: &Path) -> Option<Vec<String>> {
    let base = git_stdout(root, &["merge-base", "HEAD", "origin/master"])
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let mut files = BTreeSet::new();
    let mut saw_diff = false;
    if let Some(base) = base
        && let Some(text) = git_stdout(root, &["diff", "--name-only", &format!("{base}...HEAD")])
    {
        saw_diff = true;
        files.extend(
            text.lines()
                .filter(|line| !line.is_empty())
                .map(str::to_string),
        );
    }
    for args in [
        &["diff", "--name-only"][..],
        &["diff", "--name-only", "--cached"][..],
    ] {
        if let Some(text) = git_stdout(root, args) {
            saw_diff = true;
            files.extend(
                text.lines()
                    .filter(|line| !line.is_empty())
                    .map(str::to_string),
            );
        }
    }
    saw_diff.then(|| files.into_iter().collect())
}

fn git_stdout(root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

/// The fast per-edit gate: guard, fmt, then only the changed host packages.
fn changed_steps(workspace: &Workspace, packages: &[&Package]) -> Vec<Step> {
    let root = &workspace.root;
    let host: Vec<&Package> = packages
        .iter()
        .copied()
        .filter(|package| package.target.is_none())
        .collect();
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
    ];
    if host.is_empty() {
        for name in ["clippy (changed)", "tests (changed, parallel)"] {
            steps.push(Step {
                name: name.to_string(),
                kind: StepKind::Skipped("no changed host-testable packages".to_string()),
            });
        }
    } else {
        steps.push(command_step(
            "clippy (changed)",
            root,
            "cargo",
            changed_clippy_args(&host),
        ));
        steps.push(changed_tests_step(root, &host));
    }
    steps.push(Step {
        name: "profiles".to_string(),
        kind: StepKind::Profiles,
    });
    steps
}

fn changed_clippy_args(packages: &[&Package]) -> Vec<String> {
    let mut args = vec!["clippy".to_string()];
    for package in packages {
        args.push("-p".to_string());
        args.push(package.name.clone());
    }
    args.extend([
        "--all-targets".to_string(),
        "--".to_string(),
        "-D".to_string(),
        "warnings".to_string(),
    ]);
    args
}

fn changed_tests_step(root: &Path, packages: &[&Package]) -> Step {
    let nextest = have_nextest();
    let mut args = if nextest {
        vec![
            "nextest".to_string(),
            "run".to_string(),
            "--status-level=fail".to_string(),
            "--no-fail-fast".to_string(),
        ]
    } else {
        vec!["test".to_string()]
    };
    for package in packages {
        args.push("-p".to_string());
        args.push(package.name.clone());
    }
    command_step(
        if nextest {
            "tests (changed, parallel)"
        } else {
            "tests (changed)"
        },
        root,
        "cargo",
        args,
    )
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
        let steps = build_steps(&workspace(), true, false).unwrap();
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
        let steps = build_steps(&workspace(), false, false).unwrap();
        let kernel = steps
            .iter()
            .find(|step| step.name == "clippy kernel x86_64")
            .unwrap();
        assert!(matches!(kernel.kind, StepKind::Command(_)));
    }

    #[test]
    fn normal_mode_runs_aarch64_kernel_clippy() {
        let steps = build_steps(&workspace(), false, false).unwrap();
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

    #[test]
    fn profiles_step_is_part_of_the_gate() {
        let steps = build_steps(&workspace(), false, false).unwrap();
        let profiles = steps
            .iter()
            .find(|step| step.name == "profiles")
            .expect("the gate has a profiles step");
        assert!(matches!(profiles.kind, StepKind::Profiles));
    }

    #[test]
    fn provenance_step_runs_strict() {
        let root = std::env::temp_dir().join(format!(
            "raven-xtask-check-provenance-{}",
            std::process::id()
        ));
        let crate_dir = root.join("crates/raven-provenance");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(crate_dir.join("Cargo.toml"), "# stub\n").unwrap();
        let workspace = Workspace {
            root: root.clone(),
            packages: Vec::new(),
        };
        let steps = build_steps(&workspace, false, false).unwrap();
        let _ = std::fs::remove_dir_all(&root);

        let provenance = steps
            .iter()
            .find(|step| step.name == "provenance")
            .expect("provenance step exists");
        let StepKind::Command(spec) = &provenance.kind else {
            panic!("provenance step is not a command");
        };
        assert_eq!(spec.program, "cargo");
        assert!(
            spec.args.iter().any(|arg| arg == "--strict"),
            "make check must run provenance with --strict: {:?}",
            spec.args
        );
    }

    #[test]
    fn independent_command_steps_overlap() {
        let dir =
            std::env::temp_dir().join(format!("raven-xtask-check-overlap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let left = dir.join("left");
        let right = dir.join("right");
        let job = |mine: PathBuf, other: PathBuf| {
            thread::spawn(move || {
                std::fs::write(&mine, b"x").unwrap();
                let deadline = Instant::now() + std::time::Duration::from_secs(5);
                while !other.exists() {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    thread::sleep(std::time::Duration::from_millis(2));
                }
                true
            })
        };
        let one = job(left.clone(), right.clone());
        let two = job(right, left);
        assert!(one.join().unwrap(), "the first step never saw the second");
        assert!(two.join().unwrap(), "the second step never saw the first");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
