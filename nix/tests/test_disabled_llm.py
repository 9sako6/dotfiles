import json
from pathlib import Path
import subprocess
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


if __name__ == "__main__":
    unittest.main()
