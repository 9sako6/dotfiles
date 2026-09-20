import errno
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
import sys
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
        (self.root / "nix/inventory.nix").write_text('''{ configuration, publicSource, ... }: {
          source = publicSource;
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

    def settings(self, environment=None):
        return subprocess.run(
            [str(TARGET / "debug/dotfiles"), "settings"],
            env={**os.environ, "DOTFILES_DIR": str(self.root), **(environment or {})},
            capture_output=True, text=True, timeout=60,
        )

    def rows(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        rows = {}
        decoder = json.JSONDecoder()
        lines = iter(result.stdout.split("\n\npackages", 1)[0].splitlines())
        self.assertEqual(next(lines), "settings")
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

    def package_fixture(self):
        (self.root / "nix/inventory.nix").write_text('''{ configuration, publicSource, ... }: {
          source = publicSource;
          packages = [
            { name = "fixture-package"; manager = "mise"; declared = "1.0.0";
              lookup = { kind = "mise"; tool = "fixture-package"; }; }
            { name = "unavailable"; manager = "Homebrew (nix-darwin)";
              declared = "—"; lookup = null; }
          ];
          system = []; services = []; tools = [];
          localllm = configuration.localllm; timeZone = "UTC";
        }''')
        commands = self.root / "commands"
        commands.mkdir()
        mise = commands / "mise"
        mise.write_text('''#!/bin/sh
printf invoked > "$DOTFILES_DIR/lookup-started"
while [ ! -f "$DOTFILES_DIR/lookup-release" ]; do sleep 0.02; done
printf '2.0.0\\n'
''')
        mise.chmod(0o755)
        return {"PATH": f"{commands}:{os.environ['PATH']}"}

    def terminal_settings(self, width, color, interact=None, environment=None,
                          stdin_terminal=True, term="xterm-256color", expected_code=0):
        master, slave = pty.openpty()
        try:
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 64, width, 0, 0))
            attributes = termios.tcgetattr(slave)
            state = self.root / "terminal-state"
            state.unlink(missing_ok=True)
            chunks = []

            def attach_terminal():
                os.setsid()
                fcntl.ioctl(1, termios.TIOCSCTTY, 0)

            with os.fdopen(slave, "wb") as terminal:
                process = subprocess.Popen(
                    [sys.executable, "-c", '''
from pathlib import Path
import subprocess, sys, termios
result = subprocess.run(sys.argv[2:])
Path(sys.argv[1]).write_text(repr(termios.tcgetattr(1)))
sys.exit(result.returncode)
''', str(state), str(TARGET / "debug/dotfiles"), "settings"],
                    env={
                        **os.environ, "DOTFILES_DIR": str(self.root),
                        "TERM": term, "CLICOLOR_FORCE": str(int(color)),
                        "NO_COLOR": "" if color else "1",
                        **(environment or {}),
                    },
                    stdin=terminal if stdin_terminal else subprocess.DEVNULL,
                    stdout=terminal, stderr=subprocess.PIPE, text=True,
                    preexec_fn=attach_terminal,
                )
                quit_sent = False
                completed_at = None
                try:
                    deadline = time.monotonic() + 30
                    while process.poll() is None or select.select([master], [], [], 0)[0]:
                        if time.monotonic() > deadline:
                            self.fail("terminal output timed out")
                        if select.select([master], [], [], 0.05)[0]:
                            try:
                                chunk = os.read(master, 65536)
                            except OSError as error:
                                if error.errno == errno.EIO:
                                    break
                                raise
                            if not chunk:
                                break
                            chunks.append(chunk)
                        frame = b"".join(chunks).rsplit(b"\x1b[2J", 1)[-1]
                        if interact:
                            interact(process, master, frame)
                        elif b"q quit" in frame and b"checking latest" not in frame:
                            if completed_at is None:
                                completed_at = time.monotonic()
                            if not quit_sent and time.monotonic() - completed_at >= 0.15:
                                self.assertIsNone(process.poll(), "viewer closed after lookup")
                                os.write(master, b"q")
                                quit_sent = True
                    _, errors = process.communicate(timeout=5)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(timeout=5)
                self.assertEqual(state.read_text(), repr(attributes))
            self.assertEqual(process.returncode, expected_code, errors)
            output = b"".join(chunks).decode().replace("\r\n", "\n")
            if stdin_terminal and term != "dumb":
                self.assertTrue(output.startswith("\x1b[?1049h"), output)
                self.assertTrue(output.endswith("\x1b[?1049l"), output)
                if not interact:
                    self.assertTrue(quit_sent, "viewer exited before q")
                frame = output.rsplit("\x1b[2J", 1)[-1].split("\x1b[64;1H", 1)[0]
                return re.sub(r"\x1b\[\d+;1H", "\n", frame).strip("\n")
            return output
        finally:
            os.close(master)

    def test_piped_output_skips_latest_lookups_and_terminal_controls(self):
        environment = self.package_fixture()
        result = self.settings({**environment, "CLICOLOR_FORCE": "1", "FORCE_COLOR": "1"})
        self.rows(result)
        self.assertIn("current", result.stdout)
        self.assertIn("1.0.0", result.stdout)
        self.assertIn("Homebrew (nix-darwin)", result.stdout)
        self.assertNotIn("latest", result.stdout)
        self.assertNotIn("checking", result.stdout)
        self.assertNotIn("\x1b", result.stdout)
        self.assertEqual(result.stdout.count("\npackages\n"), 1)
        self.assertFalse((self.root / "lookup-started").exists())

    def test_noninteractive_terminal_falls_back_to_plain_output(self):
        environment = self.package_fixture()
        for options in [{"stdin_terminal": False}, {"term": "dumb"}]:
            with self.subTest(options=options):
                output = self.terminal_settings(80, color=True, environment=environment, **options)
                self.assertIn("current", output)
                self.assertIn("1.0.0", output)
                self.assertNotIn("latest", output)
                self.assertNotIn("\x1b", output)
                self.assertFalse((self.root / "lookup-started").exists())

    def test_viewer_shows_configuration_before_lookup_and_stays_open_after_results(self):
        environment = self.package_fixture()
        released = False
        completed_at = None
        quit_sent = False

        def interact(process, terminal, frame):
            nonlocal released, completed_at, quit_sent
            if not released and b"checking latest" in frame and b"q quit" in frame:
                self.assertIn(b"settings", frame)
                self.assertIn(b"1.0.0", frame)
                self.assertIn(b"current", frame)
                (self.root / "lookup-release").touch()
                released = True
            if released and b"2.0.0" in frame and b"checking latest" not in frame:
                self.assertIn(b"error: unsupported source", frame)
                if completed_at is None:
                    completed_at = time.monotonic()
                if not quit_sent and time.monotonic() - completed_at >= 0.15:
                    self.assertIsNone(process.poll(), "viewer closed after lookup")
                    os.write(terminal, b"q")
                    quit_sent = True

        output = self.terminal_settings(160, color=False, interact=interact, environment=environment)
        self.assertTrue(quit_sent)
        self.assertIn("2.0.0", output)
        self.assertIn("Homebrew (nix-darwin)", output)
        self.assertTrue((self.root / "lookup-started").exists())

    def test_quit_and_interrupt_cancel_pending_lookups_and_restore_terminal(self):
        environment = self.package_fixture()
        for key, code in [(b"q", 0), (b"\x1b", 0), (b"\x03", 130)]:
            with self.subTest(key=key):
                (self.root / "lookup-started").unlink(missing_ok=True)
                sent_at = None

                def interact(process, terminal, frame):
                    nonlocal sent_at
                    if sent_at is None and b"q quit" in frame and (self.root / "lookup-started").exists():
                        os.write(terminal, key)
                        sent_at = time.monotonic()

                self.terminal_settings(160, color=False, interact=interact,
                                       environment=environment, expected_code=code)
                self.assertIsNotNone(sent_at)
                self.assertLess(time.monotonic() - sent_at, 2)

    def test_viewer_can_resize_and_scroll_back_to_settings_after_lookup(self):
        stage = 0

        def interact(process, terminal, frame):
            nonlocal stage
            if b"q quit" not in frame or b"checking latest" in frame:
                return
            if stage == 0:
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 80, 0, 0))
                stage = 1
            elif stage == 1 and b"\x1b[12;1H" in frame:
                os.write(terminal, b"\x1b[F")
                stage = 2
            elif stage == 2 and b"default model" in frame:
                os.write(terminal, b"\x1b[H")
                stage = 3
            elif stage == 3 and b"\x1b[1;1Hsettings" in frame:
                os.write(terminal, b"q")
                stage = 4

        self.terminal_settings(160, color=False, interact=interact)
        self.assertEqual(stage, 4)

    def test_terminal_header_and_multiline_arrays_preserve_values_at_any_width(self):
        paths = ["a/" + "x" * 18, "b/" + "y" * 18, "c/" + "z" * 18]
        (self.root / "dotfiles.toml").write_text(
            f"copy = {json.dumps(paths)}\n"
            '[localllm]\nmodels = ["qwen3.8-27b-4bit"]\n'
        )
        narrow = self.terminal_settings(80, color=True)
        self.assertEqual(narrow.splitlines()[0], "\x1b[35m\x1b[3msettings\x1b[0m")
        header = narrow.splitlines()[1]
        for label in ["key", "value", "source"]:
            self.assertIn(f"\x1b[35m\x1b[3m{label}\x1b[0m", header)
        plain = re.sub(r"\x1b\[[0-9;]*m", "", narrow)
        self.assertEqual(plain.splitlines()[1].split(), ["key", "value", "source"])
        self.assertTrue(all(len(line) <= 80 for line in plain.split("\n\npackages", 1)[0].splitlines()))
        self.assertNotIn(paths[0], plain.splitlines()[2])
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
