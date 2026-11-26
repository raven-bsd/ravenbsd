#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit checks for the 0.1.0 UFS gate driver (no QEMU)."""

import unittest

import gate


class StripAnsiTest(unittest.TestCase):
    def test_prompt_colour_is_removed(self):
        self.assertEqual(
            gate.strip_ansi(b"\x1b[38;5;183mraven# \x1b[0m\r\n"),
            "raven# \n",
        )

    def test_boot_screen_codes_are_removed(self):
        text = b"\x1b[2J\x1b[01;01H\x1b[1;36mR A V E N   B S D\x1b[0m\n"
        self.assertEqual(gate.strip_ansi(text), "R A V E N   B S D\n")


class ViolationTest(unittest.TestCase):
    def test_panic_line_is_a_violation(self):
        text = (
            "raven# \n"
            "thread 'main' (1) panicked at library/std/src/thread/local.rs:428:25:\n"
            "cannot access a Thread Local Storage value during or after destruction\n"
        )
        self.assertIn("panicked", gate.violation(text))

    def test_missing_ufs_root_is_a_violation(self):
        line = "svcd: root pivot: /: no UFS root device (no such file or directory)"
        self.assertEqual(gate.violation(line), line)

    def test_benign_boot_lines_are_clean(self):
        text = "000.00  resolvd             warn\nsvcd: root pivoted to UFS at /\n"
        self.assertIsNone(gate.violation(text))


class QemuArgsTest(unittest.TestCase):
    def args(self, accel, mutation="none"):
        return gate.qemu_args(
            "x86_64",
            accel,
            "/tmp/esp.img",
            "/tmp/root.img",
            "/tmp/vars.fd",
            "/tmp/code.fd",
            mutation,
        )

    def test_kvm_cell(self):
        argv = self.args("kvm")
        self.assertEqual(argv[0], "qemu-system-x86_64")
        self.assertIn("-enable-kvm", argv)
        self.assertIn("host", argv)
        self.assertIn("if=none,id=ufs,format=raw,file=/tmp/root.img", argv)
        self.assertIn("format=raw,file=/tmp/esp.img", argv)

    def test_tcg_cell(self):
        argv = self.args("tcg")
        self.assertNotIn("-enable-kvm", argv)
        self.assertIn("max", argv)

    def test_root_readonly_mutation(self):
        argv = self.args("kvm", "root-readonly")
        self.assertIn("if=none,id=ufs,format=raw,readonly=on,file=/tmp/root.img", argv)

    def test_unknown_accelerator_is_rejected(self):
        with self.assertRaises(ValueError):
            self.args("xen")

    def test_unknown_arch_is_rejected(self):
        with self.assertRaises(ValueError):
            gate.qemu_args("aarch64", "tcg", "e", "d", "v", "c", "none")

    def test_unknown_mutation_is_rejected(self):
        with self.assertRaises(ValueError):
            self.args("kvm", "no-login")


class ImageMarkerTest(unittest.TestCase):
    def test_marker_present(self):
        import tempfile

        with tempfile.NamedTemporaryFile() as handle:
            handle.write(b"\0" * 4096 + b"S85M-RW-OK" + b"\0" * 4096)
            handle.flush()
            self.assertTrue(gate.image_contains(handle.name, b"S85M-RW-OK"))

    def test_marker_absent(self):
        import tempfile

        with tempfile.NamedTemporaryFile() as handle:
            handle.write(b"\0" * 4096)
            handle.flush()
            self.assertFalse(gate.image_contains(handle.name, b"S85M-RW-OK"))


class ParseArgsTest(unittest.TestCase):
    base = [
        "--arch", "x86_64",
        "--accel", "kvm",
        "--esp", "/tmp/esp.img",
        "--disk", "/tmp/root.img",
        "--root", "/tmp/product",
        "--out", "/tmp/evidence",
    ]

    def test_defaults(self):
        args = gate.parse_args(self.base)
        self.assertEqual(args.mutation, "none")
        self.assertEqual(args.total_timeout, 300)
        self.assertEqual(gate.artifact_suffix(args), "x86_64-kvm")

    def test_mutation_suffix(self):
        args = gate.parse_args(self.base + ["--mutation", "root-readonly"])
        self.assertEqual(gate.artifact_suffix(args), "x86_64-kvm-root-readonly")

    def test_accel_is_required(self):
        with self.assertRaises(SystemExit):
            gate.parse_args(
                ["--arch", "x86_64", "--esp", "e", "--disk", "d", "--root", "r", "--out", "o"]
            )


if __name__ == "__main__":
    unittest.main(verbosity=2)
