import errno
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
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
                        interact(process, master, output)
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

    def test_progress_is_visible_during_captured_evaluation_and_stops_before_confirmation(self):
        environment = {
            **self.environment, "DOTFILES_TEST_REVIEW_APPLY": "1",
            "DOTFILES_TEST_REVIEW_PROGRESS": "1",
        }
        process = subprocess.Popen(
            self.command, env=environment, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        try:
            self.assertTrue(select.select([process.stderr], [], [], 3)[0])
            first = os.read(process.stderr.fileno(), 65536)
            self.assertIn(b"dotfiles: Evaluating system...\n", first)
            self.assertTrue(select.select([process.stderr], [], [], 11)[0])
            heartbeat = os.read(process.stderr.fileno(), 65536)
            self.assertRegex(heartbeat, rb"Evaluating system\.\.\. \(\d+s\)")
            output = b""
            deadline = time.monotonic() + 5
            while b"Apply this system plan?" not in output:
                self.assertLess(time.monotonic(), deadline)
                if select.select([process.stdout], [], [], 0.1)[0]:
                    output += os.read(process.stdout.fileno(), 65536)
            process.stdin.write(b"yes\n")
            process.stdin.flush()
            tail, errors = process.communicate(timeout=3)
            self.assertEqual(process.returncode, 0, errors.decode())
            self.assertIn(b"Evaluating system (", errors)
            self.assertNotIn(b"Evaluating system...", errors)
            self.assertNotIn(b"\x1b", first + heartbeat + errors)
            self.assertNotIn(b"dotfiles:", output + tail)
            self.assertTrue((self.root / "activation-requested").exists())
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=3)
            for stream in [process.stdin, process.stdout, process.stderr]:
                stream.close()

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
        (self.root / "copy.json").write_text('{"settings": "settings"}')
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

    def test_terminal_prints_entire_colored_diff_once_without_input(self):
        output = self.terminal()
        self.assertIn("\x1b[31m- package-000", output)
        self.assertIn("\x1b[32m+ package-000", output)
        plain = re.sub(r"\x1b\[[0-9;]*m", "", output)
        self.assertNotIn("\x1b", plain)
        self.assertNotIn("q close", plain)
        self.assertNotIn("latest", plain)
        self.assertNotIn("unchanged", plain)
        self.assertEqual(plain.count("packages\n"), 1)
        for index in range(30):
            self.assertEqual(plain.count(f"- package-{index:03}"), 1)
            self.assertEqual(plain.count(f"+ package-{index:03}"), 1)
        header = next(line for line in plain.splitlines() if line.strip().startswith("name "))
        self.assertGreaterEqual(header.index("manager") - header.index("name"), 34)

    def test_apply_confirms_after_printing_the_whole_diff(self):
        for answer, code in [(b"no\n", 1), (b"yes\n", 0)]:
            with self.subTest(answer=answer):
                confirmed = False
                activation = self.root / "activation-requested"
                activation.unlink(missing_ok=True)

                def interact(process, terminal, output):
                    nonlocal confirmed
                    if not confirmed and b"Apply this system plan?" in output:
                        self.assertIn(b"+ package-029", output)
                        self.assertNotIn(b"q close", output)
                        self.assertNotIn(b"\x1b[?1049", output)
                        self.assertFalse(activation.exists())
                        os.write(terminal, answer)
                        confirmed = True

                self.terminal(interact, apply=True, expected_code=code)
                self.assertTrue(confirmed)
                self.assertEqual(activation.exists(), answer == b"yes\n")

    def test_terminal_output_does_not_require_terminal_input(self):
        for options in [{"stdin_terminal": False}, {"term": "dumb"}]:
            with self.subTest(options=options):
                output = self.terminal(**options)
                plain = re.sub(r"\x1b\[[0-9;]*m", "", output)
                self.assertIn("- package-000", plain)
                self.assertIn("+ package-029", plain)
                self.assertNotIn("\x1b", plain)


if __name__ == "__main__":
    unittest.main()
