import json
from pathlib import Path
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
NIX = ["nix", "--extra-experimental-features", "nix-command flakes"]
FIXED = ["--no-update-lock-file", "--no-write-lock-file"]


class ArtifactTests(unittest.TestCase):
    def test_registered_root_keeps_packages_alive(self):
        evaluated = subprocess.run(
            [*NIX, "eval", "--json", *FIXED,
             ".#checks.aarch64-darwin.artifacts.fixtureRoot", "--apply",
             "root: { derivation = root.drvPath; output = root.outPath; }"],
            cwd=REPOSITORY, capture_output=True, text=True, check=True, timeout=180,
        )
        fixture = json.loads(evaluated.stdout)
        with tempfile.TemporaryDirectory(prefix="dotfiles-resource-test-") as temporary:
            root = Path(temporary).resolve() / "current"
            subprocess.run(
                [*NIX, "build", *FIXED, "--out-link", str(root), fixture["derivation"] + "^out"],
                cwd=REPOSITORY, capture_output=True, text=True, check=True, timeout=300,
            )
            self.assertEqual(str(root.resolve()), fixture["output"])
            roots = subprocess.run(
                ["nix-store", "--query", "--roots", fixture["output"]],
                capture_output=True, text=True, check=True, timeout=60,
            ).stdout.splitlines()
            self.assertIn(str(root), {line.split(" -> ", 1)[0] for line in roots})
            closure = subprocess.run(
                ["nix-store", "--query", "--requisites", fixture["output"]],
                capture_output=True, text=True, check=True, timeout=60,
            ).stdout.splitlines()
            for relative in ["bin/nightlight", "share/anki-connect/__init__.py"]:
                target = (root / relative).resolve()
                package = "/nix/store/" + target.parts[3]
                self.assertIn(package, closure)
                self.assertTrue(target.is_file())


if __name__ == "__main__":
    unittest.main()
