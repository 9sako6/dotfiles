import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]


class BuildTests(unittest.TestCase):
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

    def test_missing_outputs_are_built_and_cached_outputs_survive_missing_recipes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            expression = root / "fixture.nix"
            expression.write_text('''builtins.derivation {
              name = "dotfiles-build-fixture";
              system = builtins.currentSystem;
              builder = "/bin/sh";
              args = [ "-c" "printf '%s' \\"$payload\\" > \\"$out\\"" ];
              payload = ''' + json.dumps(str(root)) + "; }")
            result = subprocess.run(
                ["nix", "build", "--impure", "--dry-run", "--json", "--no-substitute",
                 "--file", str(expression)],
                capture_output=True, text=True, timeout=30, check=True,
            )
            derivation = json.loads(result.stdout)[0]
            output = Path(derivation["outputs"]["out"])
            self.assertFalse(output.exists())

            def realize(build):
                (root / "build.json").write_text(json.dumps(build))
                result = subprocess.run(
                    [self.executable, "--exact", "system::tests::build_fixture", "--nocapture"],
                    env={**os.environ, "DOTFILES_TEST_BUILD_ROOT": str(root)},
                    capture_output=True, text=True, timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual((root / "result").resolve(), output)
                self.assertEqual(output.read_text(), str(root))
            realize(derivation)
            realize(derivation | {"drvPath": "/nix/store/00000000000000000000000000000000-missing.drv"})
            realize(derivation | {"outputs": {"out": str(root)}})


if __name__ == "__main__":
    unittest.main()
