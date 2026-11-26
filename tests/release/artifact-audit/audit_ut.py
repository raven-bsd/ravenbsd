# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85j contract tests for the release artifact and licence audit. These tests name
# the behaviour the audit must provide: a resolved release manifest for one
# architecture, a licence and notice for every staged artifact, exact failure
# names, and a scaffold C/C++ implementation scan that ignores public headers.

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import audit


PRODUCT = Path(__file__).resolve().parents[3]


class ProfileResolution(unittest.TestCase):
    def test_every_profile_parses_with_the_release_reader(self):
        profiles = sorted((PRODUCT / "profiles").glob("*.toml"))
        profiles += sorted((PRODUCT / "profiles").glob("*.d/*.toml"))
        self.assertGreater(len(profiles), 50)
        for path in profiles:
            audit.parse_toml_subset(path)

    def test_parent_and_sorted_fragments_resolve_in_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            profiles = root / "profiles"
            (profiles / "child.d").mkdir(parents=True)
            (profiles / "parent.toml").write_text(
                'name = "parent"\n'
                "[initfs]\n"
                'dynamic = true\n'
                "programs = [\n"
                '    { package = "one", path = "/bin/one" },\n'
                "]\n"
                "[boot]\n"
                'cmdline = "console=ttyS0"\n'
                'log_level = "info"\n',
                encoding="utf-8",
            )
            (profiles / "child.toml").write_text(
                'name = "child"\n'
                'extends = "parent"\n'
                "[[initfs.files]]\n"
                'src = "etc/child"\n'
                'path = "/etc/child"\n'
                "mode = 0o644\n",
                encoding="utf-8",
            )
            (profiles / "child.d" / "10-a.toml").write_text(
                "[[initfs.programs]]\n"
                'package = "two"\n'
                'path = "/bin/two"\n',
                encoding="utf-8",
            )
            (profiles / "child.d" / "05-b.toml").write_text(
                "[[initfs.programs]]\n"
                'package = "three"\n'
                'path = "/bin/three"\n',
                encoding="utf-8",
            )
            programs, files, dynamic, _release = audit.load_profile(profiles, "child")
            self.assertEqual(
                [p["package"] for p in programs], ["one", "three", "two"]
            )
            self.assertEqual([f["path"] for f in files], ["/etc/child"])
            self.assertTrue(dynamic)

    def test_duplicate_stage_path_is_rejected_with_both_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            profiles = root / "profiles"
            (profiles / "dup.d").mkdir(parents=True)
            (profiles / "dup.toml").write_text(
                'name = "dup"\n'
                "[[initfs.programs]]\n"
                'package = "one"\n'
                'path = "/bin/one"\n'
                "[boot]\n"
                'cmdline = "console=ttyS0"\n'
                'log_level = "info"\n',
                encoding="utf-8",
            )
            (profiles / "dup.d" / "10.toml").write_text(
                "[[initfs.files]]\n"
                'src = "etc/one"\n'
                'path = "/bin/one"\n',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(audit.AuditError, "duplicate initfs path"):
                audit.load_profile(profiles, "dup")


class ManifestVerification(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.packages = {"app": {"dir": "userland/bin/app", "license": "BSD-3-Clause"}}
        self.imports = [
            {
                "path": "addons/ssh/openssh",
                "upstream": "openssh-portable",
                "license": "ISC",
                "tier": "A",
            }
        ]
        (self.root / "userland/bin/app").mkdir(parents=True)
        (self.root / "addons/ssh/openssh").mkdir(parents=True)
        (self.root / "addons/ssh/openssh/LICENCE").write_text("ISC\n", encoding="utf-8")
        self.expected = [
            {
                "kind": "program",
                "stage_path": "/bin/app",
                "source": "userland/bin/app",
                "license": "BSD-3-Clause",
                "notice": "-",
            },
            {
                "kind": "addon",
                "stage_path": "/sbin/sshd",
                "source": "addons/ssh/openssh",
                "license": "ISC",
                "notice": "openssh-portable",
            },
        ]

    def tearDown(self):
        self.tmp.cleanup()

    def test_complete_manifest_passes(self):
        failures = audit.verify_rows(
            self.root, list(self.expected), self.expected, self.packages, self.imports
        )
        self.assertEqual(failures, [])

    def test_missing_artifact_is_named(self):
        rows = [self.expected[0]]
        failures = audit.verify_rows(
            self.root, rows, self.expected, self.packages, self.imports
        )
        self.assertTrue(any("/sbin/sshd" in line for line in failures), failures)

    def test_removed_notice_is_named(self):
        rows = [dict(row) for row in self.expected]
        rows[1]["notice"] = "-"
        failures = audit.verify_rows(
            self.root, rows, self.expected, self.packages, self.imports
        )
        self.assertTrue(any("/sbin/sshd" in line and "notice" in line for line in failures))

    def test_unlicensed_program_is_named(self):
        rows = [dict(row) for row in self.expected]
        rows[0]["license"] = ""
        failures = audit.verify_rows(
            self.root, rows, self.expected, self.packages, self.imports
        )
        self.assertTrue(any("/bin/app" in line and "licence" in line for line in failures))

    def test_unregistered_addon_is_named(self):
        rows = [dict(row) for row in self.expected]
        rows[1]["source"] = "addons/ssh/missing"
        failures = audit.verify_rows(
            self.root, rows, self.expected, self.packages, self.imports
        )
        self.assertTrue(any("/sbin/sshd" in line for line in failures))

    def test_manifest_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "manifest.tsv"
            audit.write_manifest(path, self.expected, "x86_64")
            loaded = audit.read_manifest(path)
        self.assertEqual(loaded, self.expected)


class CImplementationScan(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def _scan(self, paths):
        for rel in paths:
            path = self.root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("int main(void) { return 0; }\n", encoding="utf-8")
        return audit.scan_c_implementation(self.root, paths)

    def test_scaffold_implementation_is_a_hit(self):
        hits = self._scan(["kernel/src/arch/x86_64/model.c"])
        self.assertEqual(hits, ["kernel/src/arch/x86_64/model.c"])

    def test_test_program_is_not_scaffold(self):
        hits = self._scan(["libc/ravenlibc/tests/c/libc-hello.c"])
        self.assertEqual(hits, [])

    def test_addon_and_vendored_c_is_not_scaffold(self):
        hits = self._scan(
            ["addons/ssh/openssh/main.c", "userland/brush/vendor/nix/test/hello.c"]
        )
        self.assertEqual(hits, [])

    def test_public_header_is_not_an_implementation_hit(self):
        hits = self._scan(["libc/ravenlibc/include/stdio.h"])
        self.assertEqual(hits, [])

    def test_cpp_implementation_under_scaffold_is_a_hit(self):
        hits = self._scan(["userland/bin/foo/src/impl.cpp"])
        self.assertEqual(hits, ["userland/bin/foo/src/impl.cpp"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
