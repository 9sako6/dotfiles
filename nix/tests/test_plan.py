import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]


class PlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        result = subprocess.run(
            ["cargo", "test", "--locked", "--manifest-path", str(REPOSITORY / "cli/Cargo.toml"),
             "--no-run", "--bin", "dotfiles", "--message-format=json"],
            capture_output=True, text=True, timeout=120, check=True,
        )
        artifacts = [json.loads(line) for line in result.stdout.splitlines()]
        cls.executable = next(item["executable"] for item in artifacts
                              if item.get("executable") and item.get("profile", {}).get("test"))

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        source = self.root / "source"
        (source / "home").mkdir(parents=True)
        (source / "home/apm.yml").write_text("dependencies:\n  apm: []\n")
        for generation, version in [("before", "1.0.0"), ("after", "2.0.0")]:
            destination = self.root / generation
            destination.mkdir()
            inventory = {
                "schemaVersion": 2, "source": str(source),
                "packages": [
                    {"name": f"package-{index:03}", "manager": "Nix", "declared": version,
                     "lookup": {"kind": "must-not-fetch"}}
                    for index in range(30)
                ] + [{"name": "unchanged", "manager": "Nix", "declared": "1", "lookup": None}],
                "system": [], "services": [], "tools": [], "timeZone": "UTC",
                "localllm": {"enabled": False, "default_model": None},
            }
            (destination / "dotfiles-inventory.json").write_text(json.dumps(inventory))
        self.command = [self.executable, "--exact", "system::tests::review_fixture", "--nocapture", "--quiet"]
        self.environment = {
            **os.environ, "DOTFILES_TEST_REVIEW_ROOT": str(self.root),
            "TERM": "xterm-256color", "CLICOLOR_FORCE": "1", "NO_COLOR": "",
        }
        self.environment.pop("DOTFILES_TEST_REVIEW_APPLY", None)

    def unchanged(self):
        (self.root / "after/dotfiles-inventory.json").unlink()
        (self.root / "after").rmdir()
        (self.root / "after").symlink_to(self.root / "before", target_is_directory=True)

    def terminal(self, interact=None, apply=False, expected_code=0, term="xterm-256color",
                 stdin_terminal=True):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 100, 0, 0))
        original = termios.tcgetattr(slave)
        state = self.root / "terminal-state"
        state.unlink(missing_ok=True)
        environment = {**self.environment, "TERM": term}
        if apply:
            environment["DOTFILES_TEST_REVIEW_APPLY"] = "1"

        def attach_terminal():
            os.setsid()
            fcntl.ioctl(1, termios.TIOCSCTTY, 0)

        process = subprocess.Popen(
            [sys.executable, "-c", """
from pathlib import Path
import subprocess, sys, termios
result = subprocess.run(sys.argv[2:])
Path(sys.argv[1]).write_text(repr(termios.tcgetattr(1)))
sys.exit(result.returncode)
""", str(state), *self.command], env=environment,
            stdin=slave if stdin_terminal else subprocess.DEVNULL,
            stdout=slave, stderr=subprocess.PIPE, preexec_fn=attach_terminal,
        )
        output = b""
        try:
            deadline = time.monotonic() + 10
            while process.poll() is None or select.select([master], [], [], 0)[0]:
                self.assertLess(time.monotonic(), deadline, output.decode(errors="replace"))
                if select.select([master], [], [], 0.05)[0]:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError as error:
                        if error.errno == errno.EIO:
                            break
                        raise
                    if not chunk:
                        break
                    output += chunk
                    if interact:
                        interact(process, master, output.rsplit(b"\x1b[2J", 1)[-1])
            _, errors = process.communicate(timeout=3)
            self.assertEqual(process.returncode, expected_code, errors.decode())
            self.assertEqual(state.read_text(), repr(original))
            return output.decode().replace("\r\n", "\n")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)
            os.close(master)
            os.close(slave)

    def test_piped_preview_contains_only_changed_rows_without_latest_or_controls(self):
        result = subprocess.run(self.command, env=self.environment, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("- package-000", result.stdout)
        self.assertIn("+ package-000", result.stdout)
        self.assertIn("1.0.0", result.stdout)
        self.assertIn("2.0.0", result.stdout)
        for unwanted in ["unchanged", "latest", "\x1b", "no resource changes"]:
            self.assertNotIn(unwanted, result.stdout)

    def test_unchanged_plan_is_silent_and_does_not_open_a_viewer(self):
        self.unchanged()
        result = subprocess.run(self.command, env=self.environment, capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.replace("running 1 test", "").strip(), "")
        self.assertEqual(self.terminal().replace("running 1 test", "").strip(), "")

    def test_unchanged_apply_neither_confirms_nor_requests_activation(self):
        self.unchanged()
        environment = {**self.environment, "DOTFILES_TEST_REVIEW_APPLY": "1"}
        for answer in ["", "yes\n"]:
            with self.subTest(answer=answer):
                result = subprocess.run(self.command, env=environment, input=answer,
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.replace("running 1 test", "").strip(), "")
                self.assertFalse((self.root / "activation-requested").exists())
        self.assertEqual(self.terminal(apply=True).replace("running 1 test", "").strip(), "")
        self.assertFalse((self.root / "activation-requested").exists())

    def test_changed_apply_requires_yes_before_requesting_activation(self):
        result = subprocess.run(
            self.command, env={**self.environment, "DOTFILES_TEST_REVIEW_APPLY": "1"},
            input="yes\n", capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertLess(result.stdout.index("+ package-000"), result.stdout.index("Apply this system plan?"))
        self.assertTrue((self.root / "activation-requested").exists())

    def test_different_build_with_identical_inventory_still_shows_its_deployment(self):
        (self.root / "after/dotfiles-inventory.json").write_bytes(
            (self.root / "before/dotfiles-inventory.json").read_bytes()
        )
        for generation, revision in [("before", "old-revision"), ("after", "new-revision")]:
            (self.root / generation / "darwin-version.json").write_text(
                json.dumps({"configurationRevision": revision})
            )
        result = subprocess.run(
            self.command, env={**self.environment, "DOTFILES_TEST_REVIEW_APPLY": "1"},
            input="no\n", capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("- system", result.stdout)
        self.assertIn("old-revision", result.stdout)
        self.assertIn("+ system", result.stdout)
        self.assertIn("new-revision", result.stdout)
        self.assertIn("Apply this system plan?", result.stdout)
        self.assertFalse((self.root / "activation-requested").exists())

    def test_copy_changes_are_visible_even_when_the_generation_is_unchanged(self):
        self.unchanged()
        (self.root / "copy.json").write_text('["settings"]')
        (self.root / "source/home/settings").write_text("desired")
        (self.root / "home").mkdir()
        target = self.root / "home/settings"
        target.write_text("current")
        environment = {**self.environment, "DOTFILES_TEST_REVIEW_APPLY": "1"}
        result = subprocess.run(self.command, env=environment, input="no\n",
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertIn("- ~/settings", result.stdout)
        self.assertIn("+ ~/settings", result.stdout)
        self.assertIn("Apply this system plan?", result.stdout)
        self.assertFalse((self.root / "activation-requested").exists())
        target.write_text("desired")
        result = subprocess.run(self.command, env=environment, input="yes\n",
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.replace("running 1 test", "").strip(), "")
        self.assertFalse((self.root / "activation-requested").exists())

    def test_diff_viewer_scrolls_resizes_and_closes_without_reprinting(self):
        stage = 0

        def interact(process, terminal, frame):
            nonlocal stage
            if b"q close" not in frame:
                return
            if stage == 0:
                self.assertIn(b"\x1b[31m- package-000", frame)
                self.assertIn(b"\x1b[32m+ package-000", frame)
                self.assertNotIn(b"latest", frame)
                os.write(terminal, b"f")
                stage = 1
            elif stage == 1 and b"package-000" not in frame:
                os.write(terminal, b"b")
                stage = 2
            elif stage == 2 and b"package-000" in frame:
                os.write(terminal, b"G")
                stage = 3
            elif stage == 3 and b"package-029" in frame:
                fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", 16, 80, 0, 0))
                os.killpg(process.pid, signal.SIGWINCH)
                stage = 4
            elif stage == 4 and b"\x1b[16;1H" in frame:
                os.write(terminal, b"g")
                stage = 5
            elif stage == 5 and b"package-000" in frame:
                os.write(terminal, b"q")
                stage = 6

        output = self.terminal(interact)
        self.assertEqual(stage, 6)
        self.assertEqual(output.count("\x1b[?1049h"), 1)
        self.assertTrue(output.endswith("\x1b[?1049l"))

    def test_apply_confirms_after_viewer_closes_and_ctrl_c_never_confirms(self):
        for key, code in [(b"q", 1), (b"\x03", 130)]:
            with self.subTest(key=key):
                sent = False
                confirmed = False

                def interact(process, terminal, frame):
                    nonlocal sent, confirmed
                    if not sent and b"q close" in frame:
                        self.assertNotIn(b"Apply this system plan?", frame)
                        os.write(terminal, key)
                        sent = True
                    if sent and not confirmed and b"Apply this system plan?" in frame:
                        self.assertIn(b"\x1b[?1049l", frame)
                        os.write(terminal, b"no\n")
                        confirmed = True

                output = self.terminal(interact, apply=True, expected_code=code)
                self.assertTrue(sent)
                self.assertEqual(confirmed, key == b"q")
                self.assertIn("\x1b[?1049l", output)

    def test_noninteractive_terminals_get_plain_diff(self):
        for options in [{"stdin_terminal": False}, {"term": "dumb"}]:
            with self.subTest(options=options):
                output = self.terminal(**options)
                self.assertIn("- package-000", output)
                self.assertIn("+ package-000", output)
                self.assertNotIn("\x1b", output)


if __name__ == "__main__":
    unittest.main()
