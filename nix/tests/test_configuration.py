import json
from pathlib import Path
import subprocess
import tempfile
import unittest


class ConfigurationDiagnosticsTests(unittest.TestCase):
    def test_invalid_configuration_reports_reasons_without_values(self):
        repository = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            public = root / "dotfiles.toml"
            local = root / "dotfiles.local.toml"
            public.write_text("copy = []\n")
            local.write_text("unknown = 'secret-do-not-print'\n[private]\npath = 123456789\n")
            expression = f'''
                let public = builtins.getFlake {json.dumps("git+" + repository.as_uri())};
                in (import {json.dumps(str(repository / "nix/configuration.nix"))} {{
                    inherit (public.inputs.nixpkgs) lib;
                    publicFile = {json.dumps(str(public))};
                    localFile = {json.dumps(str(local))};
                }}).config
            '''
            result = subprocess.run(
                ["nix", "--extra-experimental-features", "nix-command flakes", "eval", "--impure", "--json", "--expr", expression],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("dotfiles configuration is invalid", result.stderr)
            self.assertIn("dotfiles.local.toml: private.path: invalid type", result.stderr)
            self.assertIn("dotfiles.local.toml: unknown: unknown key", result.stderr)
            self.assertNotIn("secret-do-not-print", result.stderr)
            self.assertNotIn("123456789", result.stderr)
            self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
