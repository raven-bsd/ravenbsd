#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit tests for toolchain/gen-libc-consts.py (S724)."""

import importlib.util
import pathlib
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "toolchain/gen-libc-consts.py"


def load_tool():
    spec = importlib.util.spec_from_file_location("gen_libc_consts", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


tool = load_tool()


class ModelValueTest(unittest.TestCase):
    def model(self, text):
        with tempfile.TemporaryDirectory() as temp:
            abi = pathlib.Path(temp)
            (abi / "t.toml").write_text(text)
            return tool.Model(abi)

    def test_alias_or_and_shift_resolve(self):
        model = self.model(
            'schema = 1\ngroup = "g"\n'
            'consts = [\n'
            '  { name = "A", value = 3 },\n'
            '  { name = "B", alias = "A" },\n'
            '  { name = "C", or = [4, "B"] },\n'
            '  { name = "D", shl = [1, 5] },\n'
            ']\n'
        )
        self.assertEqual(model.value("B"), 3)
        self.assertEqual(model.value("C"), 7)
        self.assertEqual(model.value("D"), 32)

    def test_alias_cycle_is_rejected(self):
        model = self.model(
            'schema = 1\ngroup = "g"\n'
            'consts = [\n  { name = "A", alias = "B" },\n  { name = "B", alias = "A" },\n]\n'
        )
        self.assertIsNone(model.value("A"))


class TargetTest(unittest.TestCase):
    def test_literal_alias_and_shift_values(self):
        target = tool.Target(
            "pub const A: c_int = 4;\n"
            "pub const B: c_int = A;\n"
            "pub const C: c_int = 1 << 10;\n"
            "pub const D: c_int = B | C;\n"
            "pub const E: c_ulong = !0;\n",
            "test",
        )
        self.assertEqual(target.consts["B"]["value"], 4)
        self.assertEqual(target.consts["C"]["value"], 1024)
        self.assertEqual(target.consts["D"]["value"], 1028)
        self.assertEqual(target.consts["E"]["value"], 2**64 - 1)

    def test_cfg_duplicate_is_not_a_single_row(self):
        target = tool.Target(
            '#[cfg(target_arch = "aarch64")]\npub const A: c_int = 1;\n'
            '#[cfg(target_arch = "x86_64")]\npub const A: c_int = 2;\n',
            "test",
        )
        self.assertTrue(target.consts["A"]["cfg"])


class ProfileTest(unittest.TestCase):
    def setUp(self):
        self.model = tool.Model(tool.ABI)
        self.vendor = tool.read_text(tool.VENDOR, "vendor")
        self.target = tool.Target(self.vendor, "vendor")
        self.profile = tool.build_profile(self.model, self.target)

    def test_generation_is_deterministic(self):
        first = tool.render_profile(self.profile)
        second = tool.render_profile(tool.build_profile(self.model, self.target))
        self.assertEqual(first, second)

    def test_every_frozen_constant_has_a_model_value(self):
        for name, row in self.profile["constants"].items():
            self.assertEqual(self.model.value(name), row["value"], name)

    def test_mutation_of_one_rust_value_is_rejected(self):
        text = self.vendor.replace(
            "pub const SIGUSR1: c_int = 30;", "pub const SIGUSR1: c_int = 10;", 1
        )
        self.assertNotEqual(text, self.vendor)
        mutated = tool.Target(text, "mutated")
        failures = tool.check_target(self.profile, mutated)
        self.assertTrue(any("SIGUSR1" in failure for failure in failures), failures)

    def test_mutation_of_one_profile_value_is_rejected(self):
        mutated = dict(self.profile)
        mutated["constants"] = dict(self.profile["constants"])
        mutated["constants"]["SIGUSR1"] = dict(mutated["constants"]["SIGUSR1"], value=10)
        failures = tool.check_profile(mutated, self.model, self.target)
        self.assertTrue(any("SIGUSR1" in failure for failure in failures), failures)

    def test_removing_a_frozen_row_is_rejected(self):
        mutated = dict(self.profile)
        mutated["constants"] = dict(self.profile["constants"])
        del mutated["constants"]["EAGAIN"]
        failures = tool.check_profile(mutated, self.model, self.target)
        self.assertTrue(any("EAGAIN" in failure for failure in failures), failures)


class PatchTest(unittest.TestCase):
    def setUp(self):
        self.model = tool.Model(tool.ABI)
        self.vendor = tool.read_text(tool.VENDOR, "vendor")
        self.target = tool.Target(self.vendor, "vendor")
        self.profile = tool.build_profile(self.model, self.target)
        self.patch = tool.read_text(tool.PATCH, "patch")

    def test_vendor_and_patch_agree(self):
        failures = tool.check_patch(
            self.profile,
            self.vendor,
            self.patch,
            tool.spawn_section(tool.SPAWN_PATCH),
        )
        self.assertEqual(failures, [])

    def test_stale_patch_body_is_rejected(self):
        body = tool.patch_body(self.patch, tool.RAVENBSD_PATH)
        stale = body.replace("pub const SIGUSR1: c_int = 30;", "pub const SIGUSR1: c_int = 10;", 1)
        self.assertNotEqual(stale, body)
        start, end = tool.patch_section(self.patch, tool.RAVENBSD_PATH)
        section = tool.rewritten_section(self.patch[start:end], stale)
        stale_patch = self.patch[:start] + section + self.patch[end:]
        failures = tool.check_patch(
            self.profile, self.vendor, stale_patch, tool.spawn_section(tool.SPAWN_PATCH)
        )
        self.assertTrue(any("SIGUSR1" in failure for failure in failures), failures)

    def test_regenerated_section_round_trips(self):
        body = tool.patch_body(self.patch, tool.RAVENBSD_PATH)
        start, end = tool.patch_section(self.patch, tool.RAVENBSD_PATH)
        section = tool.rewritten_section(self.patch[start:end], body)
        self.assertEqual(tool.patch_body(section + "\n", tool.RAVENBSD_PATH), body)


if __name__ == "__main__":
    unittest.main()
