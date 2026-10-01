"""Evaluate the frozen-input seam and derivation graph; never build a model."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
NIX = ["nix", "--extra-experimental-features", "nix-command flakes"]
FIXED = ["--no-update-lock-file", "--no-write-lock-file"]


class ArtifactInputTests(unittest.TestCase):
    def evaluate(self, local, operation="artifacts"):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            local_file = root / "local.toml"
            local_file.write_text(local)
            manifest = root / "inputs.json"
            # Deliberately omit host-only directory/user/revision fields and use
            # an unreadable private source. Artifact evaluation needs none of them.
            manifest.write_text(json.dumps({
                "localFile": str(local_file),
                "privateSource": str(root / "missing-private-flake"),
                "publicSource": str(REPOSITORY),
            }))
            return subprocess.run(
                [*NIX, "eval", "--json", "--impure", *FIXED,
                 "--file", str(REPOSITORY / "nix/host-input.nix")],
                env={**os.environ, "DOTFILES_INPUT_MANIFEST": str(manifest),
                     "DOTFILES_INPUT_OPERATION": operation},
                cwd=REPOSITORY, capture_output=True, text=True, timeout=180,
            )

    def test_merged_selection_without_host_or_private_evaluation(self):
        result = self.evaluate('''
[localllm]
enabled = true
models = ["qwen3.8-27b-4bit"]
default_model = "qwen3.8-27b-4bit"
[private]
path = "/missing/private"
''')
        self.assertEqual(result.returncode, 0, result.stderr)
        output = json.loads(result.stdout)
        artifacts = {entry["id"]: entry for entry in output["manifestData"]["artifacts"]}
        self.assertEqual(set(artifacts), {"anki-connect", "localllm"})
        self.assertEqual(artifacts["localllm"]["model"], "qwen3.8-27b-4bit")
        self.assertEqual(artifacts["localllm"]["relativePath"], "bin/localllm")
        self.assertEqual(artifacts["anki-connect"]["homeTarget"],
                         "Library/Application Support/Anki2/addons21/anki-connect")
        for path in [output["manifest"], output["root"], output["output"],
                     *(item["storePath"] for item in artifacts.values())]:
            self.assertTrue(path.startswith("/nix/store/"), path)

    def test_invalid_merged_configuration_is_rejected_before_build(self):
        result = self.evaluate('[localllm]\nenabled = true\nmodels = ["unknown"]\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("configuration is invalid", result.stderr)

    def test_disabled_derivation_graph_excludes_llm_and_system(self):
        expression = '''
          let
            public = builtins.getFlake ("git+file://" + builtins.getEnv "ARTIFACT_TEST_REPO");
            configuration = (import (public.outPath + "/nix/configuration.nix") {
              inherit (public.inputs.nixpkgs) lib;
              publicFile = builtins.toFile "artifact-disabled.toml" "copy = []";
            }).config;
          in (public.lib.mkArtifacts { inherit configuration; }).root
        '''
        result = subprocess.run(
            [*NIX, "derivation", "show", "--recursive", "--impure", *FIXED,
             "--expr", expression],
            cwd=REPOSITORY, env={**os.environ, "ARTIFACT_TEST_REPO": str(REPOSITORY)},
            capture_output=True, text=True, check=True, timeout=180,
        )
        graph = json.loads(result.stdout)
        self.assertTrue(graph)
        self.assertTrue(any(d.get("env", {}).get("name") == "dotfiles-artifacts"
                            for d in graph.values()))
        forbidden = []
        for path, derivation in graph.items():
            env = derivation.get("env", {})
            name = env.get("name", "")
            urls = env.get("url", "") + env.get("urls", "")
            if name.startswith(("darwin-system-", "home-manager-generation",
                                "dotfiles-localllm-", "localllm", "mlx-", "mlx_",
                                "opencode-")) or "huggingface.co/mlx-community/" in urls:
                forbidden.append(path)
        self.assertEqual(forbidden, [])


if __name__ == "__main__":
    unittest.main()
