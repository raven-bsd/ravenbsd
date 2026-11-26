#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit tests for the Pi 4 0.1.0 SD image gate.

Run: python3 tests/boot/pi4-image/run_ut.py
"""

from __future__ import annotations

import importlib.util
import struct
import sys
import unittest
import zlib
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUN = HERE / "run.py"

FAT_TYPE = "28732ac11ff8d211ba4b00a0c93ec93b"
UFS_TYPE = "b67c6e51cf6ed6118ff800022d09712b"
SECTOR = 512


def load_runner():
    spec = importlib.util.spec_from_file_location("pi4_image_run", RUN)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {RUN}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def guid(text: str) -> bytes:
    return bytes.fromhex(text)


def build_gpt(parts, damaged: str = "") -> bytes:
    """Build a minimal two-partition GPT image for parser tests."""
    total = 1024
    data = bytearray(total * SECTOR)
    data[446:462] = struct.pack("<B3sB3sII", 0, b"\x00\x02\x00", 0xEE, b"\xff\xff\xff", 1, total - 1)
    data[510:512] = b"\x55\xaa"
    header = bytearray(92)
    header[0:8] = b"EFI PART"
    struct.pack_into("<I", header, 8, 0x00010000)
    struct.pack_into("<I", header, 12, 92)
    struct.pack_into("<Q", header, 24, 1)
    struct.pack_into("<Q", header, 32, total - 1)
    struct.pack_into("<Q", header, 40, 34)
    struct.pack_into("<Q", header, 48, total - 34)
    header[56:72] = bytes(range(16))
    struct.pack_into("<Q", header, 72, 2)
    struct.pack_into("<I", header, 80, 128)
    struct.pack_into("<I", header, 84, 128)
    entries = bytearray(128 * 128)
    for index, (type_guid, start, end, name) in enumerate(parts):
        offset = index * 128
        entries[offset:offset + 16] = type_guid
        struct.pack_into("<Q", entries, offset + 32, start)
        struct.pack_into("<Q", entries, offset + 40, end)
        entries[offset + 56:offset + 128] = name.encode("utf-16le").ljust(72, b"\0")
    struct.pack_into("<I", header, 88, zlib.crc32(entries) & 0xFFFFFFFF)
    if damaged != "header-crc":
        check = bytearray(header[:92])
        check[16:20] = b"\0\0\0\0"
        struct.pack_into("<I", header, 16, zlib.crc32(check) & 0xFFFFFFFF)
    if damaged == "entry-crc":
        entries[0] ^= 0xFF
    backup = bytearray(header)
    struct.pack_into("<Q", backup, 24, total - 1)
    struct.pack_into("<Q", backup, 32, 1)
    data[512:512 + 92] = header
    data[2 * SECTOR:2 * SECTOR + len(entries)] = entries
    data[(total - 1) * SECTOR:(total - 1) * SECTOR + 92] = backup
    return bytes(data)


def good_parts():
    return [
        (guid(FAT_TYPE), 34, 149, "BOOT"),
        (guid(UFS_TYPE), 150, 900, "ROOT"),
    ]


CONFIG = (
    b"arm_64bit=1\nkernel=kernel8.img\nenable_uart=1\nuart_2ndstage=1\n"
    b"dtoverlay=disable-bt\ninit_uart_clock=48000000\ncore_freq=250\n"
)
CMDLINE = b"console=ttyAMA0 loglevel=info\n"


class GptTests(unittest.TestCase):
    def setUp(self):
        self.run = load_runner()

    def test_accepts_fat_and_ufs_layout(self):
        parts = self.run.parse_gpt_bytes(build_gpt(good_parts()))
        self.assertEqual(len(parts), 2)
        self.assertEqual(parts[0].type_guid, FAT_TYPE)
        self.assertEqual(parts[1].type_guid, UFS_TYPE)
        self.assertEqual(parts[0].start, 34)
        self.assertEqual(parts[1].sectors, 751)

    def test_rejects_single_partition(self):
        parts = [(guid(FAT_TYPE), 34, 149, "BOOT")]
        with self.assertRaisesRegex(self.run.VerifyError, "expected 2 GPT partitions"):
            self.run.parse_gpt_bytes(build_gpt(parts))

    def test_rejects_wrong_partition_type(self):
        parts = [
            (guid(FAT_TYPE), 34, 149, "BOOT"),
            (guid(FAT_TYPE), 150, 900, "ROOT"),
        ]
        with self.assertRaisesRegex(self.run.VerifyError, "partition types"):
            self.run.parse_gpt_bytes(build_gpt(parts))

    def test_rejects_damaged_header_crc(self):
        with self.assertRaisesRegex(self.run.VerifyError, "header CRC"):
            self.run.parse_gpt_bytes(build_gpt(good_parts(), damaged="header-crc"))

    def test_rejects_damaged_entry_crc(self):
        with self.assertRaisesRegex(self.run.VerifyError, "entry table CRC"):
            self.run.parse_gpt_bytes(build_gpt(good_parts(), damaged="entry-crc"))


class BootTests(unittest.TestCase):
    def setUp(self):
        self.run = load_runner()
        self.entries = self.run.load_boot_manifest(HERE / "boot-manifest.txt")

    def files(self):
        return {
            "config.txt": CONFIG,
            "cmdline.txt": CMDLINE,
            "kernel8.img": b"\x01" * 65536,
            "bcm2711-rpi-4-b.dtb": b"\xd0\x0d\xfe\xed" + b"\0" * 2048,
            "bootcode.bin": b"\x02" * 4096,
            "start4.elf": b"\x03" * 1048576,
            "fixup4.dat": b"\x04" * 2048,
            "LICENCE.broadcom": b"licence\n",
            "overlays/disable-bt.dtbo": b"\xd0\x0d\xfe\xed" + b"\0" * 1024,
        }

    def test_manifest_lists_every_boot_file(self):
        paths = [entry.path for entry in self.entries]
        for name in ("config.txt", "cmdline.txt", "kernel8.img", "start4.elf", "overlays/disable-bt.dtbo"):
            self.assertIn(name, paths)

    def test_accepts_complete_boot_partition(self):
        rows = self.run.verify_boot_files(self.entries, self.files())
        self.assertEqual(len(rows), len(self.entries))

    def test_missing_boot_file_is_named(self):
        files = self.files()
        del files["start4.elf"]
        with self.assertRaisesRegex(self.run.VerifyError, r"start4\.elf"):
            self.run.verify_boot_files(self.entries, files)

    def test_short_boot_file_is_named(self):
        files = self.files()
        files["bootcode.bin"] = b"\x02" * 16
        with self.assertRaisesRegex(self.run.VerifyError, "bootcode.bin"):
            self.run.verify_boot_files(self.entries, files)

    def test_config_requires_uart_lines(self):
        files = self.files()
        files["config.txt"] = CONFIG.replace(b"enable_uart=1\n", b"")
        with self.assertRaisesRegex(self.run.VerifyError, "enable_uart=1"):
            self.run.verify_boot_files(self.entries, files)

    def test_cmdline_requires_ttyama0(self):
        files = self.files()
        files["cmdline.txt"] = b"console=ttyS0 loglevel=info\n"
        with self.assertRaisesRegex(self.run.VerifyError, "console=ttyAMA0"):
            self.run.verify_boot_files(self.entries, files)

    def test_fdt_magic_is_required(self):
        files = self.files()
        files["bcm2711-rpi-4-b.dtb"] = b"not a device tree" + b"\0" * 2048
        with self.assertRaisesRegex(self.run.VerifyError, "FDT"):
            self.run.verify_boot_files(self.entries, files)


class LoginTests(unittest.TestCase):
    def setUp(self):
        self.run = load_runner()
        self.payload = {
            "/etc/passwd": b"root:x:0:0:root:/root:/bin/rsh\nraven:x:1000:1000:raven:/home/raven:/bin/rsh\n",
            "/etc/group": b"root:x:0:root\nraven:x:1000:raven\n",
            "/etc/shadow": b"root:$6$rounds=5000$abc$def:20000:0:99999:7:::\n",
            "/etc/profile": b"PATH=/bin:/sbin\nRAVEN_PROFILE=loaded\n",
            "/etc/motd": b"RavenBSD\n",
            "/etc/rc.conf": b'rsh_enable = "NO"\n',
            "/etc/mounts": b"/ ufs:\n/tmp ramfs:\n",
            "/etc/raven/root-mode": b"pivot\n",
            "/etc/raven/autologin.toml": b"[console]\nuser = \"\"\nenabled = false\n",
            "/etc/services.d/getty.toml": (
                b'name = "getty"\nexec = "/bin/getty"\nrestart = "always"\n'
            ),
        }
        self.modes = {
            "/etc/shadow": 0o100600,
            "/bin/getty": 0o100755,
            "/bin/login": 0o100755,
            "/bin/rsh": 0o100755,
            "/bin/brush": 0o100755,
        }

    def add_executables(self):
        for path in ("/bin/getty", "/bin/login", "/bin/rsh", "/bin/brush"):
            self.payload.setdefault(path, b"\x7fELF")

    def test_accepts_login_configuration_without_ssh(self):
        self.add_executables()
        summary = self.run.verify_login(self.payload, self.modes)
        self.assertEqual(summary["ssh"], "absent")

    def test_shadow_must_stay_private(self):
        self.add_executables()
        self.modes["/etc/shadow"] = 0o100644
        with self.assertRaisesRegex(self.run.VerifyError, "/etc/shadow mode"):
            self.run.verify_login(self.payload, self.modes)

    def test_root_shell_is_the_login_shell(self):
        self.add_executables()
        self.payload["/etc/passwd"] = b"root:x:0:0:root:/root:/bin/sh\n"
        with self.assertRaisesRegex(self.run.VerifyError, "/etc/passwd"):
            self.run.verify_login(self.payload, self.modes)

    def test_getty_service_is_required(self):
        self.add_executables()
        del self.payload["/etc/services.d/getty.toml"]
        with self.assertRaisesRegex(self.run.VerifyError, "getty.toml"):
            self.run.verify_login(self.payload, self.modes)

    def test_console_autologin_must_be_disabled(self):
        self.add_executables()
        self.payload["/etc/raven/autologin.toml"] = b"[console]\nenabled = true\n"
        with self.assertRaisesRegex(self.run.VerifyError, "autologin"):
            self.run.verify_login(self.payload, self.modes)

    def test_ssh_present_requires_its_configuration(self):
        self.add_executables()
        self.payload["/sbin/sshd"] = b"\x7fELF"
        self.modes["/sbin/sshd"] = 0o100755
        with self.assertRaisesRegex(self.run.VerifyError, "/etc/ssh/sshd_config"):
            self.run.verify_login(self.payload, self.modes)

    def test_ssh_present_with_config_is_accepted(self):
        self.add_executables()
        self.payload["/sbin/sshd"] = b"\x7fELF"
        self.modes["/sbin/sshd"] = 0o100755
        self.modes["/bin/ssh"] = 0o100755
        self.modes["/bin/ssh-keygen"] = 0o100755
        self.payload["/bin/ssh"] = b"\x7fELF"
        self.payload["/bin/ssh-keygen"] = b"\x7fELF"
        self.payload["/etc/ssh/sshd_config"] = (
            b"Port 22\nPermitRootLogin yes\nAllowUsers root raven\n"
            b"PasswordAuthentication yes\nPermitEmptyPasswords no\n"
        )
        self.payload["/etc/services.d/sshd.toml"] = b'name = "sshd"\n'
        self.payload["/etc/services.d/ssh-hostkeys.toml"] = b'name = "ssh-hostkeys"\n'
        summary = self.run.verify_login(self.payload, self.modes)
        self.assertEqual(summary["ssh"], "present")

    def test_mounts_must_mount_ufs_root(self):
        self.add_executables()
        self.payload["/etc/mounts"] = b"/ initfs:\n"
        with self.assertRaisesRegex(self.run.VerifyError, "/etc/mounts"):
            self.run.verify_login(self.payload, self.modes)


class SuperblockTests(unittest.TestCase):
    def setUp(self):
        self.run = load_runner()

    def test_accepts_ufs2_magic(self):
        block = bytearray(8192)
        struct.pack_into("<I", block, 1372, 0x19540119)
        self.run.check_ufs2_superblock(bytes(block))

    def test_rejects_ufs1_magic(self):
        block = bytearray(8192)
        struct.pack_into("<I", block, 1372, 0x01195401)
        with self.assertRaisesRegex(self.run.VerifyError, "UFS2"):
            self.run.check_ufs2_superblock(bytes(block))


if __name__ == "__main__":
    unittest.main()
