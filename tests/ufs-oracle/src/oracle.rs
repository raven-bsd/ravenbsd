// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Optional host oracles: NetBSD `fsck_ffs` and `tools/ufs-oracle/check-ufs.py`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::model::{Kind, Model};

/// Locate an oracle `fsck_ffs` via `UFS_FSCK_FFS` or `UFS_ORACLE_DIR`.
fn fsck_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("UFS_FSCK_FFS") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let dir = PathBuf::from(std::env::var_os("UFS_ORACLE_DIR")?);
    [dir.join("bin/fsck_ffs"), dir.join("fsck_ffs")]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// Run `fsck_ffs -n` on a cleanly unmounted image when the oracle exists.
pub fn fsck(image: &Path) -> Result<String, String> {
    let Some(binary) = fsck_binary() else {
        return Ok("absent".to_string());
    };
    let output = Command::new(&binary)
        .arg("-n")
        .arg(image)
        .output()
        .map_err(|error| format!("run fsck_ffs: {error}"))?;
    if output.status.success() {
        Ok("ok".to_string())
    } else {
        Err(format!(
            "fsck_ffs failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn sha256_hex(data: &[u8]) -> Result<Option<String>, String> {
    let mut child = match Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("spawn sha256sum: {error}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(data)
            .map_err(|error| format!("write sha256sum stdin: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait sha256sum: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "sha256sum failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let hash = text
        .split_whitespace()
        .next()
        .ok_or("sha256sum emitted no hash")?;
    Ok(Some(hash.to_string()))
}

fn write_manifest(
    model: &Model,
    vectors: &Path,
    manifest_path: &Path,
) -> Result<Option<String>, String> {
    let baseline = vectors.join("expected/ufs2-b4096.manifest");
    let Some(text) = fs::read_to_string(&baseline).ok() else {
        return Ok(Some("absent:baseline".to_string()));
    };
    let mut lines = Vec::new();
    let mut root_seen = false;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('|').collect();
        if fields.len() != 9 {
            return Err(format!("bad baseline manifest line: {line}"));
        }
        if fields[0] == "/" {
            let nlink = fields[8]
                .parse::<u32>()
                .map_err(|_| "bad baseline root nlink")?;
            lines.push(format!("{}|{}", fields[..8].join("|"), nlink + 1));
            root_seen = true;
        } else {
            lines.push(line.to_string());
        }
    }
    if !root_seen {
        return Err("baseline manifest has no root entry".to_string());
    }

    let mut model_lines = Vec::new();
    let mut paths: Vec<&Vec<u8>> = model.paths.keys().collect();
    paths.sort();
    for path in paths {
        let ino = model.paths.get(path).ok_or("model path without inode")?;
        let node = model.inode(*ino).ok_or("model path without node")?;
        let full = if path.is_empty() {
            "/b64".to_string()
        } else {
            format!("/b64/{}", String::from_utf8_lossy(path))
        };
        let nlink = model.nlink(*ino).ok_or("model path without nlink")?;
        let (kind, size, hash, link) = match node.kind {
            Kind::Dir => (
                "dir".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
            ),
            Kind::File => {
                let Some(hash) = sha256_hex(&node.data)? else {
                    return Ok(Some("absent:sha256sum".to_string()));
                };
                (
                    "file".to_string(),
                    node.data.len().to_string(),
                    hash,
                    "-".to_string(),
                )
            }
            Kind::Symlink => (
                "symlink".to_string(),
                node.target.len().to_string(),
                "-".to_string(),
                String::from_utf8_lossy(&node.target).into_owned(),
            ),
        };
        model_lines.push(format!(
            "{full}|{kind}|{:04o}|{}|{}|{size}|{hash}|{link}|{nlink}",
            node.mode, node.uid, node.gid
        ));
    }
    lines.extend(model_lines);
    fs::write(manifest_path, lines.join("\n") + "\n")
        .map_err(|error| format!("write {}: {error}", manifest_path.display()))?;
    Ok(None)
}

/// Compare the image with the independent model using the Python oracle reader.
pub fn check_ufs(
    image: &Path,
    model: &Model,
    vectors: &Path,
    manifest_path: &Path,
) -> Result<String, String> {
    let script = vectors.join("../..").join("tools/ufs-oracle/check-ufs.py");
    if !script.is_file() {
        return Ok("absent:check-ufs".to_string());
    }
    if let Some(status) = write_manifest(model, vectors, manifest_path)? {
        return Ok(status);
    }
    let output = match Command::new("python3")
        .arg(&script)
        .arg(image)
        .arg(manifest_path)
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok("absent:python3".to_string());
        }
        Err(error) => return Err(format!("run check-ufs.py: {error}")),
    };
    if output.status.success() {
        Ok("ok".to_string())
    } else {
        Err(format!(
            "check-ufs.py failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}
