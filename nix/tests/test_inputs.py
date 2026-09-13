import json
from pathlib import Path
import subprocess
import tempfile
import unittest


class InputBoundaryTests(unittest.TestCase):
    def test_git_snapshot_contains_dirty_tracked_files_and_excludes_local_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def git(*arguments):
                return subprocess.run(["git", "-C", str(root), *arguments], check=True, capture_output=True, text=True)
            git("init", "--quiet")
            (root / "flake.nix").write_text('{ outputs = { self }: { }; }')
            (root / "tracked").write_text("before")
            git("add", "flake.nix", "tracked")
            git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture")
            reference = "git+" + root.as_uri()
            subprocess.run(["nix", "flake", "lock", reference], check=True, capture_output=True)
            (root / "tracked").write_text("after")
            (root / "dotfiles.local.toml").write_text("local-value")
            (root / "untracked-secret").write_text("fixture-secret")
            metadata = subprocess.run(["nix", "flake", "metadata", "--json", "--no-update-lock-file", "--no-write-lock-file", reference], check=True, capture_output=True)
            source = Path(json.loads(metadata.stdout)["path"])
            self.assertEqual((source / "tracked").read_text(), "after")
            self.assertFalse((source / "dotfiles.local.toml").exists())
            self.assertFalse((source / "untracked-secret").exists())


if __name__ == "__main__":
    unittest.main()
