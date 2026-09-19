import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import shutil
import struct
import subprocess
import tempfile
import termios
import time
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", REPOSITORY / "cli/target"))


class SettingsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        tracked = subprocess.check_output(
            ["git", "-C", str(REPOSITORY), "ls-files", "-z"], text=True,
        )
        for name in filter(None, tracked.split("\0")):
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(REPOSITORY / name, destination)
        (self.root / "dotfiles.toml").write_text("copy = []\n")
        (self.root / "nix/inventory.nix").write_text('''{ configuration, ... }: {
          packages = []; system = []; services = []; tools = [];
          localllm = configuration.localllm; timeZone = "UTC";
        }''')
        (self.root / "home").mkdir(exist_ok=True)
        (self.root / "home/apm.yml").write_text("dependencies:\n  apm: []\n")
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
            env={**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"},
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
        lines = iter(result.stdout.split("\n\npackages", 1)[0].splitlines())
        for line in lines:
            key, rest = line.split(maxsplit=1)
            if rest.startswith("["):
                source = rest[1:].strip() or None
                parts = ["["]
                for continuation in lines:
                    parts.append(continuation.strip())
                    if continuation.strip() == "]":
                        break
                value = json.loads("\n".join(parts))
            else:
                value, end = decoder.raw_decode(rest)
                source = rest[end:].strip() or None
            self.assertNotIn(key, rows)
            rows[key] = (value, source)
        self.assertEqual(list(rows), sorted(rows))
        return rows

    def terminal_settings(self, width, color):
        master, slave = pty.openpty()
        try:
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, width, 0, 0))
            chunks = []
            with os.fdopen(slave, "wb") as terminal:
                process = subprocess.Popen(
                    [str(TARGET / "debug/dotfiles"), "settings"],
                    env={
                        **os.environ, "DOTFILES_DIR": str(self.root),
                        "TERM": "xterm-256color", "CLICOLOR_FORCE": str(int(color)),
                        "NO_COLOR": "" if color else "1",
                    },
                    stdin=subprocess.DEVNULL, stdout=terminal, stderr=subprocess.PIPE, text=True,
                )
                deadline = time.monotonic() + 30
                while process.poll() is None or select.select([master], [], [], 0)[0]:
                    if time.monotonic() > deadline:
                        process.kill()
                        self.fail("terminal output timed out")
                    if select.select([master], [], [], 0.05)[0]:
                        chunks.append(os.read(master, 65536))
                _, errors = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 0, errors)
            return b"".join(chunks).decode().replace("\r\n", "\n")
        finally:
            os.close(master)

    def test_terminal_header_and_multiline_arrays_preserve_values_at_any_width(self):
        paths = ["a/" + "x" * 18, "b/" + "y" * 18, "c/" + "z" * 18]
        (self.root / "dotfiles.toml").write_text(
            f"copy = {json.dumps(paths)}\n"
            '[localllm]\nmodels = ["qwen3.8-27b-4bit"]\n'
        )
        narrow = self.terminal_settings(80, color=True)
        header = narrow.splitlines()[0]
        for label in ["key", "value", "source"]:
            self.assertIn(f"\x1b[35m\x1b[3m{label}\x1b[0m", header)
        plain = re.sub(r"\x1b\[[0-9;]*m", "", narrow)
        self.assertEqual(plain.splitlines()[0].split(), ["key", "value", "source"])
        self.assertTrue(all(len(line) <= 80 for line in plain.split("\n\npackages", 1)[0].splitlines()))
        self.assertNotIn(paths[0], plain.splitlines()[1])
        for path in paths:
            self.assertEqual(plain.count(json.dumps(path)), 1)
        wide = self.terminal_settings(160, color=False)
        self.assertNotIn("\x1b[", wide)
        piped = self.settings()
        for output in [plain, wide, piped.stdout]:
            self.assertNotIn(json.dumps(paths), output)
            self.assertRegex(output, r'localllm.models +\[ +dotfiles.toml\n +"qwen3.8-27b-4bit"\n +\]')
            for path in paths:
                self.assertEqual(output.count(json.dumps(path)), 1)
        self.assertEqual(self.rows(piped)["copy"], (paths, "dotfiles.toml"))

    def test_absent_local_file_includes_every_default(self):
        self.assertEqual(self.rows(self.settings()), {
            "copy": ([], "dotfiles.toml"),
            "localllm.default_model": (None, None),
            "localllm.enabled": (False, None),
            "localllm.models": ([], None),
            "private.path": (None, None),
        })

    def test_local_overrides_remain_visible_when_private_checkout_is_missing(self):
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
        result = self.settings()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("private.path: checkout does not exist", result.stderr)
        self.assertNotIn("\npackages\n", result.stdout)
        result.returncode = 0
        result.stderr = ""
        self.assertEqual(self.rows(result), {
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
