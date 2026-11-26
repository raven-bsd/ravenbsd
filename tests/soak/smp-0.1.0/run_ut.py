#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit tests for the S85h 0.1.0 SMP soak runner."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load_runner():
    spec = importlib.util.spec_from_file_location("s85h_run", HERE / "run.py")
    if spec is None or spec.loader is None:
        raise ImportError("cannot load run.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


RUN = load_runner()

CLEAN_X86 = (
    "loader: RavenBSD loader 0.1 (x86_64)\n"
    "RavenBSD kernel 0.1 (x86_64)\n"
    "smp: 4 cpus online\n"
    "svcd: getty up\n"
    "login: \n"
)
CLEAN_AARCH64 = (
    "loader: RavenBSD loader 0.1 (aarch64)\n"
    "RavenBSD kernel 0.1 (aarch64)\n"
    "smp: 4 cpus online\n"
    "svcd: getty up\n"
    "login: \n"
)


class ScanTests(unittest.TestCase):
    def test_clean_x86_boot(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86, 4)
        self.assertEqual(scan.login_line, "login:")
        self.assertEqual(scan.smp_line, "smp: 4 cpus online")
        self.assertEqual(scan.failures, [])

    def test_clean_aarch64_boot(self) -> None:
        scan = RUN.scan_serial(CLEAN_AARCH64, 4)
        self.assertEqual(scan.login_line, "login:")
        self.assertEqual(scan.smp_line, "smp: 4 cpus online")
        self.assertEqual(scan.failures, [])

    def test_missing_cpu_is_not_a_clean_cell(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86.replace("smp: 4 cpus online", "smp: 2 cpus online"), 4)
        self.assertIsNone(scan.smp_line)

    def test_login_missing_is_not_a_clean_cell(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86.replace("login: \n", ""), 4)
        self.assertIsNone(scan.login_line)

    def test_panic_fails(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86 + "KERNEL PANIC\n", 4)
        self.assertTrue(scan.failures)

    def test_exception_fails(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86 + "EXCEPTION: bad ELF\n", 4)
        self.assertTrue(scan.failures)

    def test_tlb_shootdown_timeout_fails(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86 + "smp: tlb shootdown timeout on cpu 3\n", 4)
        self.assertTrue(scan.failures)

    def test_triple_fault_fails(self) -> None:
        scan = RUN.scan_serial(CLEAN_X86 + "triple fault\n", 4)
        self.assertTrue(scan.failures)

    def test_service_crash_loop_is_an_observation(self) -> None:
        scan = RUN.scan_serial(CLEAN_AARCH64 + "svcd: netd crash loop, giving up\n", 4)
        self.assertEqual(scan.failures, [])
        self.assertTrue(scan.observations)


class CellTests(unittest.TestCase):
    def test_valid_cells(self) -> None:
        self.assertIsNone(RUN.cell_error("x86_64", "kvm", 4))
        self.assertIsNone(RUN.cell_error("x86_64", "tcg", 4))
        self.assertIsNone(RUN.cell_error("aarch64", "tcg", 4))

    def test_unknown_accel_is_rejected(self) -> None:
        self.assertIsNotNone(RUN.cell_error("x86_64", "bogus", 4))

    def test_zero_cpus_is_rejected(self) -> None:
        self.assertIsNotNone(RUN.cell_error("x86_64", "tcg", 0))

    def test_aarch64_kvm_is_rejected_on_this_host(self) -> None:
        self.assertIsNotNone(RUN.cell_error("aarch64", "kvm", 4))


class QemuTests(unittest.TestCase):
    def test_kvm4_argv(self) -> None:
        argv = RUN.qemu_argv("x86_64", "kvm", 4, Path("test.img"), Path("vars.fd"))
        self.assertIn("-enable-kvm", argv)
        self.assertIn("-smp", argv)
        self.assertEqual(argv[argv.index("-smp") + 1], "4")

    def test_tcg4_argv(self) -> None:
        argv = RUN.qemu_argv("x86_64", "tcg", 4, Path("test.img"), Path("vars.fd"))
        self.assertIn("-accel", argv)
        self.assertNotIn("-enable-kvm", argv)
        self.assertEqual(argv[argv.index("-smp") + 1], "4")

    def test_aarch64_tcg4_argv(self) -> None:
        argv = RUN.qemu_argv("aarch64", "tcg", 4, Path("test.img"), Path("vars.fd"))
        self.assertEqual(argv[0], "qemu-system-aarch64")
        self.assertEqual(argv[argv.index("-smp") + 1], "4")
        self.assertIn("virt,gic-version=2", argv)


if __name__ == "__main__":
    unittest.main()
