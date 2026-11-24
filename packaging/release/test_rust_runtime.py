# SPDX-License-Identifier: GPL-3.0-or-later
"""Runtime notice collection fixtures; no real compiler, build or network."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import rust_runtime

COMPILER = "rustc 1.98.1 (aaaaaaaaa 2026-09-01)"
COMMIT = "a" * 40
TARGET = "x86_64-unknown-linux-gnu"
VERBOSE = f"{COMPILER}\nbinary: rustc\ncommit-hash: {COMMIT}\ncommit-date: 2026-09-01\nhost: {TARGET}\nrelease: 1.98.1\nLLVM version: 22.1.0"


class RuntimeNoticeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sysroot = self.root / "toolchain"
        docs = self.sysroot / "share/doc/rust"
        (docs / "licenses").mkdir(parents=True)
        (docs / "COPYRIGHT-library.html").write_text("<html>Copyright notices for The Rust Standard Library</html>")
        for name in rust_runtime.LICENSES:
            (docs / "licenses" / name).write_text("Compiler supplied license " + name)
        self.builtins = self.sysroot / "lib/rustlib/src/rust/library/compiler-builtins"
        (self.builtins / "compiler-builtins").mkdir(parents=True)
        (self.builtins / "LICENSE.txt").write_text("Compiler-builtins MIT AND Apache-2.0 WITH LLVM-exception")
        (self.builtins / "compiler-builtins/Cargo.toml").write_text('[package]\nname="compiler_builtins"\nversion="0.1.160"\nlicense="MIT AND Apache-2.0 WITH LLVM-exception"\n')
        self.target_libdir = self.sysroot / "lib/rustlib" / TARGET / "lib"
        self.target_libdir.mkdir(parents=True)
        (self.target_libdir / "libstd-abcdef.rlib").write_bytes(b"runtime")
        (self.target_libdir / "libcompiler_builtins-abcdef.rlib").write_bytes(b"builtins")

        def command(*arguments):
            return {("--version",): COMPILER, ("-vV",): VERBOSE,
                    ("--print", "sysroot"): str(self.sysroot),
                    ("--print", "target-libdir", "--target", TARGET): str(self.target_libdir)}[arguments]

        self.enterContext(patch.object(rust_runtime, "_command", side_effect=command))

    def collect(self):
        return rust_runtime.runtime_entries(COMPILER, TARGET)

    def payload(self):
        output = self.root / "payload"
        output.mkdir()
        for name, data in self.collect().items():
            path = output / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        return output

    def test_collects_complete_compiler_matched_notices_and_source_provenance(self):
        entries = self.collect()
        self.assertEqual(set(entries), set(rust_runtime.NOTICE_LIMITS) | {"BUILD-INFO.json"})
        info = json.loads(entries["BUILD-INFO.json"])
        self.assertEqual(info["compiler_commit"], COMMIT)
        self.assertTrue(info["rust_source_url"].endswith(COMMIT))
        self.assertEqual(info["compiler_builtins"]["version"], "0.1.160")
        self.assertNotIn(str(self.sysroot), entries["BUILD-INFO.json"].decode())
        output = self.payload()
        self.assertEqual(rust_runtime.validate_runtime(output, COMPILER, TARGET)["compiler"], COMPILER)

    def test_wrong_compiler_is_rejected_before_source_collection(self):
        with self.assertRaisesRegex(ValueError, "does not match"):
            rust_runtime.runtime_entries("rustc 1.100.0 (bbbbbbbbb 2026-11-09)", TARGET)

    def test_missing_compiler_builtins_notice_is_not_silently_omitted(self):
        (self.builtins / "LICENSE.txt").unlink()
        with self.assertRaisesRegex(ValueError, "required Rust notice"):
            self.collect()

    def test_verifier_rejects_altered_and_extra_payload_and_wrong_target(self):
        output = self.payload()
        notice = output / "licenses/MIT.txt"
        original = notice.read_bytes()
        notice.write_bytes(b"changed notice")
        with self.assertRaisesRegex(ValueError, "Changed Rust"):
            rust_runtime.validate_runtime(output, COMPILER, TARGET)
        notice.write_bytes(original)
        extra = output / "extra-private-file"
        extra.write_bytes(b"not a license")
        with self.assertRaisesRegex(ValueError, "extra files"):
            rust_runtime.validate_runtime(output, COMPILER, TARGET)
        extra.unlink()
        with self.assertRaisesRegex(ValueError, "compiler/target"):
            rust_runtime.validate_runtime(output, COMPILER, "aarch64-unknown-linux-gnu")

    def test_collection_rejects_symlinked_legal_material(self):
        notice = self.sysroot / "share/doc/rust/licenses/MIT.txt"
        outside = self.root / "outside"
        outside.write_bytes(notice.read_bytes())
        notice.unlink()
        notice.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.collect()

    def test_binary_identity_requires_unique_actual_metadata(self):
        raw = f"Karin 0.8.0-rc.1\nCompiler: {COMPILER}\nTarget: {TARGET}\n"
        self.assertEqual(rust_runtime.binary_identity(raw), (COMPILER, TARGET))
        with self.assertRaisesRegex(ValueError, "one compiler"):
            rust_runtime.binary_identity(raw + f"Compiler: {COMPILER}\n")


if __name__ == "__main__":
    unittest.main()
