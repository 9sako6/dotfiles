import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class DisabledLlmTests(unittest.TestCase):
    def test_public_system_does_not_require_llm_downloads(self):
        root = Path(__file__).resolve().parents[2]
        result = subprocess.run(
            [
                "nix", "--extra-experimental-features", "nix-command flakes",
                "derivation", "show", "--recursive", "--offline",
                "--no-update-lock-file", "--no-write-lock-file",
                ".#darwinConfigurations.current.system",
            ],
            cwd=root, capture_output=True, text=True, check=True, timeout=120,
        )
        derivations = json.loads(result.stdout)
        self.assertTrue(derivations)
        self.assertTrue(any(
            derivation.get("env", {}).get("name", "").startswith("darwin-system-")
            for derivation in derivations.values()
        ))
        forbidden = []
        for path, derivation in derivations.items():
            environment = derivation.get("env", {})
            name = environment.get("name", "")
            urls = environment.get("url", "") + environment.get("urls", "")
            if name.startswith(("dotfiles-localllm-", "localllm", "mlx-", "mlx_", "opencode-")) or "huggingface.co/mlx-community/" in urls:
                forbidden.append(path)
        self.assertEqual(forbidden, [], "disabled system includes dedicated LLM build inputs")

    def test_projected_system_has_no_public_home_cli_or_artifact_dependencies(self):
        root = Path(__file__).resolve().parents[2]
        system_files = [
            "flake.lock",
            "flake.nix",
            "nix/default.nix",
            "nix/home.nix",
            "nix/homebrew-packages.nix",
            "nix/homebrew-shellenv.zsh",
            "nix/host-flake.nix",
            "nix/inventory.nix",
            "nix/macos-settings.nix",
            "nix/system.nix",
        ]
        expression = '''
          let
            public = builtins.getFlake ("path:" + builtins.getEnv "PROJECTED_SYSTEM_SOURCE");
            host = public.lib.mkHost {
              configuration = {
                copy = [];
                localllm = { enabled = false; default_model = null; models = []; };
                private.path = null;
              };
              dotfilesDirectory = "/fixture";
              primaryUser = "fixture";
            };
          in host.system
        '''
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source"
            for name in system_files:
                destination = source / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(root / name, destination)
            self.assertFalse((source / "cli").exists())
            self.assertFalse((source / "home").exists())
            result = subprocess.run(
                ["nix", "--extra-experimental-features", "nix-command flakes",
                 "derivation", "show", "--recursive", "--impure",
                 "--no-update-lock-file", "--no-write-lock-file", "--expr", expression],
                cwd=root, env={**os.environ, "PROJECTED_SYSTEM_SOURCE": str(source)},
                capture_output=True, text=True, check=True, timeout=180,
            )
        derivations = json.loads(result.stdout)
        names = [item.get("env", {}).get("name", "") for item in derivations.values()]
        self.assertTrue(any(name.startswith("darwin-system-") for name in names))
        self.assertTrue(any(name == "anki-bin-26.05" for name in names))
        forbidden = [name for name in names if name.startswith((
            "anki-connect-", "dotfiles-artifacts", "dotfiles-localllm-", "dotfiles-source-",
            "home-manager-generation", "localllm", "nightlight-",
        ))]
        self.assertEqual(forbidden, [], "system depends on public user packages")


if __name__ == "__main__":
    unittest.main()
