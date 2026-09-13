import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", REPOSITORY / "cli/target"))


class SettingsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in [
            "bin/system-backend.sh",
            "flake.lock",
            "flake.nix",
            "lib/install-system.sh",
            "nix/configuration.nix",
            "nix/host-input.nix",
            "nix/localllm/catalog.nix",
        ]:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPOSITORY / name, destination)
        (self.root / "dotfiles.toml").write_text("copy = []\n")
        self.git("init", "--quiet")
        self.git("add", ".")
        self.git(
            "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false",
            "commit", "--quiet", "-m", "fixture",
        )

    def git(self, *arguments):
        return subprocess.run(
            ["git", "-C", str(self.root), *arguments],
            check=True, capture_output=True, text=True,
        )

    def settings(self):
        return subprocess.run(
            [str(TARGET / "debug/dotfiles"), "settings"],
            env={**os.environ, "DOTFILES_DIR": str(self.root)},
            capture_output=True, text=True, timeout=60,
        )

    def rows(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        rows = {}
        decoder = json.JSONDecoder()
        for line in result.stdout.splitlines():
            key, rest = line.split(maxsplit=1)
            value, end = decoder.raw_decode(rest)
            self.assertNotIn(key, rows)
            rows[key] = (value, rest[end:].strip() or None)
        self.assertEqual(list(rows), sorted(rows))
        return rows

    def test_absent_local_file_includes_every_default(self):
        self.assertEqual(self.rows(self.settings()), {
            "copy": ([], "dotfiles.toml"),
            "localllm.default_model": (None, None),
            "localllm.enabled": (False, None),
            "localllm.models": ([], None),
            "private.path": (None, None),
        })

    def test_local_overrides_dirty_shared_values_without_resolving_private_checkout(self):
        (self.root / "dotfiles.toml").write_text(
            'copy = ["a", "b"]\n[localllm]\nenabled = true\n'
            'models = ["qwen3.8-27b-4bit"]\ndefault_model = "qwen3.8-27b-4bit"\n'
        )
        (self.root / "dotfiles.local.toml").write_text(
            '[localllm]\nenabled = false\nmodels = []\n'
            '[private]\npath = "../missing-private"\n'
        )
        before = self.git("status", "--porcelain").stdout
        lock = (self.root / "flake.lock").read_bytes()
        self.assertEqual(self.rows(self.settings()), {
            "copy": (["a", "b"], "dotfiles.toml"),
            "localllm.default_model": ("qwen3.8-27b-4bit", "dotfiles.toml"),
            "localllm.enabled": (False, "dotfiles.local.toml"),
            "localllm.models": ([], "dotfiles.local.toml"),
            "private.path": ("../missing-private", "dotfiles.local.toml"),
        })
        self.assertEqual(self.git("status", "--porcelain").stdout, before)
        self.assertEqual((self.root / "flake.lock").read_bytes(), lock)

    def test_invalid_configuration_fails_without_partial_output_or_values(self):
        for content, error in [
            ('[private]\npath = "secret-do-not-print', "invalid TOML"),
            ('unknown = "secret-do-not-print"', "unknown key"),
            ('[localllm]\nenabled = "secret-do-not-print"', "invalid type"),
            ('copy = []', "only allowed in dotfiles.toml"),
            ('[localllm]\nenabled = true', "enabled requires a model"),
        ]:
            with self.subTest(content=content):
                (self.root / "dotfiles.local.toml").write_text(content)
                result = self.settings()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, "")
                self.assertIn(error, result.stderr)
                self.assertNotIn("secret-do-not-print", result.stderr)

    def test_unreadable_local_file_is_not_treated_as_absent(self):
        (self.root / "dotfiles.local.toml").mkdir()
        result = self.settings()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertIn("cannot read local configuration", result.stderr)


if __name__ == "__main__":
    unittest.main()
