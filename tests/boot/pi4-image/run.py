#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Build the Pi 4 0.1.0 SD image and verify its host-side boot contract.

The gate builds `--profile 0.1.0-ufs` for aarch64, then checks the GPT layout,
the FAT boot files, the UFS2 root filesystem, the serial console configuration
and the login/SSH configuration. It records the image hash and a file manifest
under `tickets/evidence/artifacts/S85o/` in the management repository. A board
boot is never claimed here; the Pi 4 hardware run belongs to the owner.

The `--mutation missing-boot-file` mode removes one boot file from a copy of the
image and must fail naming that exact file.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from dataclasses import dataclass
from pathlib import Path

HERE = Path(__file__).resolve().parent
BOOT_MANIFEST = HERE / "boot-manifest.txt"

SECTOR = 512
FAT_TYPE = "28732ac11ff8d211ba4b00a0c93ec93b"
UFS_TYPE = "b67c6e51cf6ed6118ff800022d09712b"
SBLOCK_UFS2 = 65536
UFS2_MAGIC = 0x19540119
SUPERBLOCK_MAGIC_OFFSET = 1372

IMAGE_NAME = "ravenbsd-pi4-0.1.0.img"
DEFAULT_EPOCH = "1700000000"
MUTATION_BOOT_FILE = "bootcode.bin"

CONFIG_LINES = (
    b"arm_64bit=1",
    b"kernel=kernel8.img",
    b"enable_uart=1",
    b"uart_2ndstage=1",
    b"dtoverlay=disable-bt",
    b"init_uart_clock=48000000",
    b"core_freq=250",
)
CMDLINE_CONSOLE = b"console=ttyAMA0"

# Data files the login path reads from the UFS2 root.
UFS_DATA = (
    "/etc/passwd",
    "/etc/group",
    "/etc/shadow",
    "/etc/profile",
    "/etc/motd",
    "/etc/rc.conf",
    "/etc/mounts",
    "/etc/raven/autologin.toml",
    "/etc/services.d/getty.toml",
)
# Executables the serial login path starts.
UFS_EXEC = (
    "/bin/getty",
    "/bin/login",
    "/bin/rsh",
    "/bin/brush",
)
# The SSH add-on is optional for this image; when sshd is present its policy and
# service files must be complete. S85k owns the SSH login gate itself.
UFS_SSH_DATA = (
    "/etc/ssh/sshd_config",
    "/etc/services.d/sshd.toml",
    "/etc/services.d/ssh-hostkeys.toml",
)
UFS_SSH_EXEC = (
    "/bin/ssh",
    "/bin/ssh-keygen",
)


class VerifyError(Exception):
    """A host-side image check failed; the message names the exact item."""


@dataclass(frozen=True)
class Partition:
    name: str
    type_guid: str
    start: int
    sectors: int

    @property
    def offset(self) -> int:
        return self.start * SECTOR

    @property
    def length(self) -> int:
        return self.sectors * SECTOR


@dataclass(frozen=True)
class BootEntry:
    path: str
    minimum: int
    check: str


class GptSource:
    """Random access to an image without loading it into memory."""

    def __init__(self, size: int, read):
        self.size = size
        self._read = read

    def read(self, offset: int, length: int) -> bytes:
        return self._read(offset, length)

    @classmethod
    def from_bytes(cls, data: bytes) -> "GptSource":
        return cls(len(data), lambda offset, length: data[offset:offset + length])

    @classmethod
    def from_path(cls, path: Path) -> "GptSource":
        handle = open(path, "rb")
        size = os.fstat(handle.fileno()).st_size

        def read(offset: int, length: int) -> bytes:
            handle.seek(offset)
            return handle.read(length)

        source = cls(size, read)
        source._handle = handle
        return source

    def close(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            handle.close()


def parse_gpt_bytes(data: bytes) -> list[Partition]:
    return parse_gpt(GptSource.from_bytes(data))


def parse_gpt(source: GptSource) -> list[Partition]:
    if source.size < 4096 or source.size % SECTOR != 0:
        raise VerifyError("image is not a whole number of 512-byte sectors")
    total = source.size // SECTOR
    mbr = source.read(0, 512)
    if mbr[510:512] != b"\x55\xaa":
        raise VerifyError("protective MBR signature is not 55aa")
    if mbr[446 + 4] != 0xEE:
        raise VerifyError("protective MBR does not declare a 0xEE GPT partition")

    header = source.read(512, 512)
    if header[:8] != b"EFI PART":
        raise VerifyError("primary GPT header signature missing")
    header_size = struct.unpack_from("<I", header, 12)[0]
    if header_size < 92 or header_size > 512:
        raise VerifyError("invalid GPT header size")
    stored_crc = struct.unpack_from("<I", header, 16)[0]
    check = bytearray(header[:header_size])
    check[16:20] = b"\0\0\0\0"
    if zlib.crc32(check) & 0xFFFFFFFF != stored_crc:
        raise VerifyError("primary GPT header CRC mismatch")
    current, backup = struct.unpack_from("<QQ", header, 24)
    first_usable, last_usable = struct.unpack_from("<QQ", header, 40)
    entries_lba = struct.unpack_from("<Q", header, 72)[0]
    count, entry_size = struct.unpack_from("<II", header, 80)
    if count == 0 or entry_size < 128:
        raise VerifyError("invalid GPT entry table")
    if current != 1:
        raise VerifyError("primary GPT header current LBA is not 1")
    if backup + 1 != total:
        raise VerifyError("GPT backup LBA does not match the image size")
    entries = source.read(entries_lba * SECTOR, count * entry_size)
    if len(entries) != count * entry_size:
        raise VerifyError("GPT entry table is truncated")
    if zlib.crc32(entries) & 0xFFFFFFFF != struct.unpack_from("<I", header, 88)[0]:
        raise VerifyError("GPT entry table CRC mismatch")

    backup_header = source.read(backup * SECTOR, 92)
    if backup_header[:8] != b"EFI PART":
        raise VerifyError("backup GPT header signature missing")
    if struct.unpack_from("<Q", backup_header, 24)[0] != backup:
        raise VerifyError("backup GPT header current LBA mismatch")
    if struct.unpack_from("<Q", backup_header, 32)[0] != current:
        raise VerifyError("backup GPT header backup LBA mismatch")

    parts = []
    for index in range(count):
        entry = entries[index * entry_size:(index + 1) * entry_size]
        if entry[:16] == b"\0" * 16:
            continue
        begin, end = struct.unpack_from("<QQ", entry, 32)
        name = entry[56:128].decode("utf-16le", "replace").rstrip("\0")
        parts.append(Partition(name, entry[:16].hex(), begin, end - begin + 1))
    if len(parts) != 2:
        raise VerifyError(f"expected 2 GPT partitions, found {len(parts)}")
    if parts[0].type_guid != FAT_TYPE or parts[1].type_guid != UFS_TYPE:
        raise VerifyError("GPT partition types are not FAT-boot + UFS-root")
    for part in parts:
        if part.start < first_usable or part.start + part.sectors - 1 > last_usable:
            raise VerifyError(f"partition {part.name} is outside the GPT usable range")
    if parts[0].start + parts[0].sectors > parts[1].start:
        raise VerifyError("FAT and UFS partitions overlap")
    return parts


def load_boot_manifest(path: Path) -> list[BootEntry]:
    entries = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 3:
            raise VerifyError(f"{path}:{number}: expected path, minimum bytes and check")
        try:
            minimum = int(fields[1])
        except ValueError as error:
            raise VerifyError(f"{path}:{number}: invalid minimum size") from error
        if fields[2] not in ("config", "cmdline", "kernel", "fdt", "none"):
            raise VerifyError(f"{path}:{number}: unknown check {fields[2]}")
        entries.append(BootEntry(fields[0], minimum, fields[2]))
    if not entries:
        raise VerifyError(f"{path}: empty boot manifest")
    return entries


def verify_boot_files(entries: list[BootEntry], files: dict[str, bytes]) -> list[dict]:
    rows = []
    for entry in entries:
        data = files.get(entry.path)
        if data is None:
            raise VerifyError(f"boot file missing: {entry.path}")
        if len(data) < entry.minimum:
            raise VerifyError(
                f"boot file {entry.path} is {len(data)} bytes, need >= {entry.minimum}"
            )
        if entry.check == "config":
            lines = data.split(b"\n")
            for required in CONFIG_LINES:
                if required not in lines:
                    raise VerifyError(f"config.txt missing '{required.decode()}'")
        elif entry.check == "cmdline":
            if CMDLINE_CONSOLE not in data:
                raise VerifyError("cmdline.txt lacks console=ttyAMA0")
        elif entry.check == "kernel":
            if data[:16] == b"\0" * 16:
                raise VerifyError("kernel8.img starts with zeros")
        elif entry.check == "fdt":
            if data[:4] != b"\xd0\x0d\xfe\xed":
                raise VerifyError(f"{entry.path} is not an FDT blob")
        rows.append(
            {"path": entry.path, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        )
    return rows


def _text(payload: dict[str, bytes], path: str) -> str:
    data = payload.get(path)
    if data is None:
        raise VerifyError(f"UFS path missing: {path}")
    return data.decode("utf-8", "replace")


def _lines(text: str) -> list[str]:
    return [line.strip() for line in text.splitlines()]


def verify_login(payload: dict[str, bytes], modes: dict[str, int]) -> dict:
    passwd = _lines(_text(payload, "/etc/passwd"))
    if "root:x:0:0:root:/root:/bin/rsh" not in passwd:
        raise VerifyError("/etc/passwd has no root entry with the /bin/rsh login shell")
    if not any(line.startswith("raven:") and line.endswith(":/bin/rsh") for line in passwd):
        raise VerifyError("/etc/passwd has no raven account with a login shell")

    if "root:x:0:root" not in _lines(_text(payload, "/etc/group")):
        raise VerifyError("/etc/group has no root group")

    shadow = _lines(_text(payload, "/etc/shadow"))
    if not any(line.startswith("root:$6$") for line in shadow):
        raise VerifyError("/etc/shadow has no password hash for root")
    if modes.get("/etc/shadow", 0) & 0o777 != 0o600:
        raise VerifyError(f"/etc/shadow mode is {modes.get('/etc/shadow', 0) & 0o777:04o}, need 0600")

    profile = _text(payload, "/etc/profile")
    if "RAVEN_PROFILE=loaded" not in profile:
        raise VerifyError("/etc/profile is not the login profile")

    if not _text(payload, "/etc/motd").strip():
        raise VerifyError("/etc/motd is empty")

    getty = _text(payload, "/etc/services.d/getty.toml")
    for required in ('name = "getty"', 'exec = "/bin/getty"', 'restart = "always"'):
        if required not in getty:
            raise VerifyError(f"/etc/services.d/getty.toml missing {required}")

    autologin = _text(payload, "/etc/raven/autologin.toml")
    if "[console]" not in autologin or "enabled = false" not in autologin:
        raise VerifyError("/etc/raven/autologin.toml must keep console autologin disabled")

    if 'rsh_enable = "NO"' not in _text(payload, "/etc/rc.conf"):
        raise VerifyError('/etc/rc.conf must set rsh_enable = "NO" for getty/login')

    if not any(line.startswith("/ ufs:") for line in _lines(_text(payload, "/etc/mounts"))):
        raise VerifyError("/etc/mounts does not mount ufs: on /")

    for path in UFS_EXEC:
        if path not in payload:
            raise VerifyError(f"UFS path missing: {path}")
        if modes.get(path, 0) & 0o111 == 0:
            raise VerifyError(f"{path} is not executable")

    ssh = "absent"
    if "/sbin/sshd" in payload:
        ssh = "present"
        if modes.get("/sbin/sshd", 0) & 0o111 == 0:
            raise VerifyError("/sbin/sshd is not executable")
        for path in UFS_SSH_DATA + UFS_SSH_EXEC:
            if path not in payload:
                raise VerifyError(f"UFS path missing: {path}")
        for path in UFS_SSH_EXEC:
            if modes.get(path, 0) & 0o111 == 0:
                raise VerifyError(f"{path} is not executable")
        config = _text(payload, "/etc/ssh/sshd_config")
        for required in (
            "Port 22",
            "PermitRootLogin yes",
            "AllowUsers root raven",
            "PasswordAuthentication yes",
            "PermitEmptyPasswords no",
        ):
            if required not in config:
                raise VerifyError(f"/etc/ssh/sshd_config missing {required}")
    return {"getty": "present", "root": "present", "ssh": ssh}


def check_ufs2_superblock(block: bytes) -> None:
    if len(block) < SUPERBLOCK_MAGIC_OFFSET + 4:
        raise VerifyError("UFS2 superblock is truncated")
    magic = struct.unpack_from("<I", block, SUPERBLOCK_MAGIC_OFFSET)[0]
    if magic != UFS2_MAGIC:
        raise VerifyError(f"UFS2 superblock magic is {magic:#010x}, need {UFS2_MAGIC:#010x}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_logged(argv: list[str], cwd: Path, env: dict | None, log: Path, label: str) -> None:
    log.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True)
    text = result.stdout.decode("utf-8", "replace") + result.stderr.decode("utf-8", "replace")
    with open(log, "a", encoding="utf-8") as handle:
        handle.write(f"$ {' '.join(argv)}\n{text}\nexit={result.returncode}\n")
    if result.returncode != 0:
        tail = "\n".join(text.strip().splitlines()[-20:])
        raise VerifyError(f"{label} failed (exit {result.returncode}):\n{tail}")
    sys.stdout.write(text)
    sys.stdout.flush()


def find_firmware(product_root: Path) -> Path:
    configured = os.environ.get("RAVEN_RPI_FIRMWARE")
    if configured:
        path = Path(configured)
        if path.is_dir():
            return path
        raise VerifyError(f"RAVEN_RPI_FIRMWARE is not a directory: {path}")
    for ancestor in (product_root, *product_root.parents):
        for candidate in (
            ancestor / "ravenbsd-resources" / "rpi-firmware" / "boot",
            ancestor / "ravenbsd-resources" / "rpi-firmware",
        ):
            if candidate.is_dir():
                return candidate
    raise VerifyError(
        "Raspberry Pi firmware not found; set RAVEN_RPI_FIRMWARE to the firmware boot directory"
    )


def find_management_root(product_root: Path) -> Path:
    configured = os.environ.get("RAVEN_MGMT_ROOT")
    if configured:
        path = Path(configured).expanduser().resolve()
        if path.is_dir():
            return path
        raise VerifyError(f"RAVEN_MGMT_ROOT is not a directory: {path}")
    for ancestor in (product_root, *product_root.parents):
        candidate = ancestor / "ravenbsd-prjmgmt"
        if candidate.is_dir():
            return candidate
    raise VerifyError(
        "management root not found; set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout"
    )


def fat_read(image: Path, fat: Partition, path: str) -> bytes:
    argv = ["mtype", "-i", f"{image}@@{fat.offset}", f"::/{path}"]
    result = subprocess.run(argv, capture_output=True)
    if result.returncode != 0:
        raise VerifyError(f"boot file missing: {path}")
    return result.stdout


def extract(image: Path, part: Partition, out: Path) -> None:
    with open(image, "rb") as source, open(out, "wb") as target:
        source.seek(part.offset)
        remaining = part.length
        while remaining:
            chunk = source.read(min(1024 * 1024, remaining))
            if not chunk:
                raise VerifyError(f"image truncated extracting {part.name}")
            target.write(chunk)
            remaining -= len(chunk)


def ufs(argv: list[str], image: Path, part: Partition) -> subprocess.CompletedProcess:
    return subprocess.run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "ufs-inject",
            "--",
            "--offset",
            str(part.offset),
            "--size",
            str(part.length),
            str(image),
            *argv,
        ],
        capture_output=True,
    )


def ufs_stat(image: Path, part: Partition, path: str) -> int | None:
    result = ufs(["stat", path], image, part)
    if result.returncode != 0:
        return None
    for field in result.stdout.decode("utf-8", "replace").split():
        if field.startswith("mode="):
            return int(field[len("mode="):], 8)
    raise VerifyError(f"cannot parse ufs-inject stat output for {path}")


def ufs_cat(image: Path, part: Partition, path: str) -> bytes:
    result = ufs(["cat", path], image, part)
    if result.returncode != 0:
        raise VerifyError(f"UFS path missing: {path}")
    return result.stdout


def probe_read_write(image: Path, ufs_part: Partition, work: Path) -> None:
    root = work / "root.ufs"
    extract(image, ufs_part, root)
    probe = work / "rw-probe"
    probe.write_bytes(b"pi4-image read-write probe\n")
    put = subprocess.run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "ufs-inject",
            "--",
            str(root),
            "put",
            str(probe),
            "/etc/raven/rw-probe",
            "--mode",
            "0644",
        ],
        capture_output=True,
    )
    if put.returncode != 0:
        raise VerifyError(
            "UFS2 root is not writable: "
            + put.stderr.decode("utf-8", "replace").strip().splitlines()[-1]
        )
    read = subprocess.run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "ufs-inject",
            "--",
            str(root),
            "cat",
            "/etc/raven/rw-probe",
        ],
        capture_output=True,
    )
    if read.returncode != 0 or read.stdout != probe.read_bytes():
        raise VerifyError("UFS2 root read-write probe did not read back")
    check = subprocess.run(
        ["cargo", "run", "-q", "-p", "ufs-fsck", "--", "-n", str(root)],
        capture_output=True,
    )
    if check.returncode != 0:
        raise VerifyError(
            "UFS2 root is not clean after the read-write probe: "
            + check.stderr.decode("utf-8", "replace").strip().splitlines()[-1]
        )
    root.unlink()


def verify_image(image: Path, evidence_dir: Path) -> dict:
    source = GptSource.from_path(image)
    try:
        parts = parse_gpt(source)
        check_ufs2_superblock(source.read(parts[1].offset + SBLOCK_UFS2, 8192))
        entries = load_boot_manifest(BOOT_MANIFEST)
        files = {entry.path: fat_read(image, parts[0], entry.path) for entry in entries}
    finally:
        source.close()

    boot_rows = verify_boot_files(entries, files)
    fsck = ufs(["check"], image, parts[1])
    if fsck.returncode != 0:
        tail = fsck.stderr.decode("utf-8", "replace").strip().splitlines()
        raise VerifyError("UFS2 fsck failed: " + (tail[-1] if tail else "no output"))

    payload: dict[str, bytes] = {}
    modes: dict[str, int] = {}
    for path in UFS_DATA:
        mode = ufs_stat(image, parts[1], path)
        if mode is None:
            raise VerifyError(f"UFS path missing: {path}")
        modes[path] = mode
        payload[path] = ufs_cat(image, parts[1], path)
    for path in UFS_EXEC:
        mode = ufs_stat(image, parts[1], path)
        if mode is None:
            raise VerifyError(f"UFS path missing: {path}")
        modes[path] = mode
        payload[path] = b"executable"
    if ufs_stat(image, parts[1], "/sbin/sshd") is not None:
        payload["/sbin/sshd"] = b"executable"
        modes["/sbin/sshd"] = ufs_stat(image, parts[1], "/sbin/sshd") or 0
        for path in UFS_SSH_DATA:
            mode = ufs_stat(image, parts[1], path)
            if mode is None:
                raise VerifyError(f"UFS path missing: {path}")
            modes[path] = mode
            payload[path] = ufs_cat(image, parts[1], path)
        for path in UFS_SSH_EXEC:
            mode = ufs_stat(image, parts[1], path)
            if mode is None:
                raise VerifyError(f"UFS path missing: {path}")
            modes[path] = mode
            payload[path] = b"executable"

    login = verify_login(payload, modes)

    with tempfile.TemporaryDirectory(prefix="pi4-image-rw.") as temporary:
        probe_read_write(image, parts[1], Path(temporary))

    digest = sha256_file(image)
    manifest = {
        "image": image,
        "sha256": digest,
        "bytes": image.stat().st_size,
        "parts": parts,
        "boot": boot_rows,
        "login": login,
        "superblock": f"{UFS2_MAGIC:#010x}",
    }
    write_manifest(evidence_dir, manifest)
    return manifest


def write_manifest(evidence_dir: Path, manifest: dict) -> None:
    evidence_dir.mkdir(parents=True, exist_ok=True)
    image = manifest["image"]
    lines = [
        f"image {image} sha256={manifest['sha256']} bytes={manifest['bytes']}",
        "gpt tab=ok partitions=2 fat+ufs",
    ]
    for part in manifest["parts"]:
        lines.append(f"gpt {part.name} start={part.start} sectors={part.sectors}")
    for row in manifest["boot"]:
        lines.append(f"fat {row['path']} bytes={row['bytes']} sha256={row['sha256']}")
    lines.append(f"ufs2 superblock={manifest['superblock']} fsck=clean rw=ok")
    lines.append("serial console=ttyAMA0 uart=115200-8n1")
    login = manifest["login"]
    lines.append(
        f"login getty={login['getty']} root={login['root']} ssh={login['ssh']}"
    )
    for path in UFS_DATA:
        lines.append(f"ufs {path} ok")
    for path in UFS_EXEC:
        lines.append(f"ufs {path} exec=ok")
    (evidence_dir / "manifest.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (evidence_dir / "image.sha256").write_text(
        f"{manifest['sha256']}  {image.name}\n", encoding="utf-8"
    )


def build_image(product_root: Path, image: Path, evidence_dir: Path) -> None:
    firmware = find_firmware(product_root)
    env = os.environ.copy()
    env["RAVEN_RPI_FIRMWARE"] = str(firmware)
    env.setdefault("SOURCE_DATE_EPOCH", DEFAULT_EPOCH)
    argv = [
        "cargo",
        "xtask",
        "image",
        "--arch",
        "aarch64",
        "--platform",
        "pi4",
        "--profile",
        "0.1.0-ufs",
        "--output",
        str(image),
    ]
    log = evidence_dir / "build.log"
    if log.exists():
        log.unlink()
    run_logged(argv, product_root, env, log, "Pi 4 image build")


def mutate_missing_boot_file(image: Path, evidence_dir: Path) -> int:
    if not image.is_file():
        raise VerifyError(f"image not found: {image} (run the build gate first)")
    work = Path(tempfile.mkdtemp(prefix="pi4-image-mutation."))
    try:
        mutated = work / IMAGE_NAME
        shutil.copyfile(image, mutated)
        source = GptSource.from_path(mutated)
        try:
            parts = parse_gpt(source)
        finally:
            source.close()
        fat = work / "esp.img"
        extract(mutated, parts[0], fat)
        result = subprocess.run(
            ["mdel", "-i", str(fat), f"::/{MUTATION_BOOT_FILE}"], capture_output=True
        )
        if result.returncode != 0:
            raise VerifyError(f"cannot remove {MUTATION_BOOT_FILE} from the copy")
        with open(mutated, "r+b") as handle:
            handle.seek(parts[0].offset)
            handle.write(fat.read_bytes())
        try:
            verify_image(mutated, evidence_dir / "mutation")
        except VerifyError as error:
            if MUTATION_BOOT_FILE not in str(error):
                raise
            log = evidence_dir / "mutation.log"
            log.parent.mkdir(parents=True, exist_ok=True)
            log.write_text(
                f"removed boot file: {MUTATION_BOOT_FILE}\nverification failure: {error}\n",
                encoding="utf-8",
            )
            print(f"pi4-image: mutation detected: {error}")
            return 1
        raise VerifyError(
            f"mutation not detected: image verification passed with {MUTATION_BOOT_FILE} removed"
        )
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arch", default="aarch64")
    parser.add_argument("--profile", default="0.1.0-ufs")
    parser.add_argument("--image", type=Path)
    parser.add_argument("--mutation", choices=("missing-boot-file",))
    parser.add_argument("--evidence-dir", type=Path)
    args = parser.parse_args(argv)

    product_root = HERE.parent.parent
    try:
        if args.arch != "aarch64":
            raise VerifyError("the Pi 4 image requires --arch aarch64")
        if args.profile != "0.1.0-ufs":
            raise VerifyError("the 0.1.0 Pi 4 image requires --profile 0.1.0-ufs")
        evidence_dir = args.evidence_dir or (
            find_management_root(product_root)
            / "tickets"
            / "evidence"
            / "artifacts"
            / "S85o"
        )
        image = args.image or product_root / "build" / "aarch64" / IMAGE_NAME
        if args.mutation:
            return mutate_missing_boot_file(image, evidence_dir)
        build_image(product_root, image, evidence_dir)
        manifest = verify_image(image, evidence_dir)
    except VerifyError as error:
        print(f"pi4-image: FAIL {error}", file=sys.stderr)
        return 1
    print(
        "pi4-image: PASS image={} sha256={} login_ssh={}".format(
            manifest["image"], manifest["sha256"], manifest["login"]["ssh"]
        )
    )
    print("pi4-image: board evidence is not claimed; the Pi 4 hardware boot is the owner's run")
    return 0


if __name__ == "__main__":
    sys.exit(main())
