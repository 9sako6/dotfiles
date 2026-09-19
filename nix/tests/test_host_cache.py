import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from urllib.parse import urlencode


REPOSITORY = Path(__file__).resolve().parents[2]


class HostCacheTests(unittest.TestCase):
    def test_frozen_inputs_reuse_evaluation_and_changes_invalidate_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            environment = {**os.environ, "XDG_CACHE_HOME": str(root / "cache")}

            def nix(*arguments):
                result = subprocess.run(
                    ["nix", "--extra-experimental-features", "nix-command flakes", *arguments],
                    env=environment, capture_output=True, text=True, timeout=60,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                return result

            def reference(directory):
                metadata = json.loads(nix(
                    "flake", "metadata", "--json", "--no-update-lock-file",
                    "--no-write-lock-file", "path:" + str(directory),
                ).stdout)
                return "path:" + metadata["path"] + "?" + urlencode({
                    "narHash": metadata["locked"]["narHash"],
                })

            public = root / "public"
            (public / "nix").mkdir(parents=True)
            (public / "flake.nix").write_text('''{
              outputs = { self }: {
                inputs.nixpkgs.lib = {};
                lib.mkHost = arguments:
                  let
                    make = name: builtins.derivation {
                      inherit name;
                      system = "aarch64-darwin";
                      builder = "/bin/sh";
                      payload = builtins.toJSON {
                        inherit (arguments) configuration configurationRevision dotfilesDirectory primaryUser;
                        private = if arguments.privateFlake == null then null
                          else arguments.privateFlake.value;
                      };
                    };
                  in builtins.trace "host-cache-fixture-evaluated" {
                    pkgs.writeText = name: text: builtins.derivation {
                      inherit name text;
                      system = "aarch64-darwin";
                      builder = "/bin/sh";
                    };
                    homebrewBrewfile = make "Brewfile";
                    system = make "system";
                  };
              };
            }''')
            (public / "nix/configuration.nix").write_text('''{ lib, publicFile, localFile }: {
              config = {
                public = builtins.fromTOML (builtins.readFile publicFile);
                local = if localFile == null then null
                  else builtins.fromTOML (builtins.readFile localFile);
              };
            }''')
            (public / "dotfiles.toml").write_text('value = "original"')
            (public / "nix/inventory.nix").write_text('''{ configuration, ... }:
              builtins.trace "inventory-fixture-evaluated" configuration
            ''')
            private = root / "private"
            private.mkdir()
            (private / "flake.nix").write_text('{ outputs = {self}: { value = "private"; }; }')
            inputs = {
                "directory": "/fixture/${directory}",
                "localFile": None,
                "privateFlake": None,
                "publicFlake": reference(public),
                "publicRevision": "fixture-revision",
                "user": "fixture",
            }

            def evaluate(changes=None, local=None):
                with tempfile.TemporaryDirectory(dir=root) as workspace:
                    workspace = Path(workspace)
                    shutil.copyfile(REPOSITORY / "nix/host-flake.nix", workspace / "flake.nix")
                    (workspace / "inputs.json").write_text(json.dumps(inputs | (changes or {})))
                    if local is not None:
                        (workspace / "dotfiles.local.toml").write_text(local)
                    source = nix("store", "add-path", "--name", "source", str(workspace)).stdout.strip()
                    result = nix(
                        "build", "--dry-run", "--json", "--no-update-lock-file",
                        "--no-write-lock-file", source + "#system", source + "#brewfile",
                    )
                    self.assertFalse((workspace / "flake.lock").exists())
                    paths = [build["drvPath"] for build in json.loads(result.stdout)]
                    self.assertEqual(len(paths), 2)
                    self.assertTrue(paths[0].endswith("-system.drv"))
                    self.assertTrue(paths[1].endswith("-Brewfile.drv"))
                    return source, paths, "host-cache-fixture-evaluated" in result.stderr

            original_source, original_paths, evaluated = evaluate()
            self.assertTrue(evaluated)
            self.assertEqual(evaluate(), (original_source, original_paths, False))
            inventory = nix("build", "--dry-run", "--json", "--no-write-lock-file", "--no-update-lock-file", original_source + "#inventory")
            self.assertIn("inventory-fixture-evaluated", inventory.stderr)
            cached_inventory = nix("build", "--dry-run", "--json", "--no-write-lock-file", "--no-update-lock-file", original_source + "#inventory")
            self.assertEqual(json.loads(inventory.stdout), json.loads(cached_inventory.stdout))
            self.assertNotIn("inventory-fixture-evaluated", cached_inventory.stderr)
            original_private = reference(private)
            (private / "flake.nix").write_text('{ outputs = {self}: { value = "changed"; }; }')
            seen_paths = {tuple(original_paths)}
            for changes, local in [
                ({"directory": "/another-fixture"}, None),
                ({"privateFlake": original_private}, None),
                ({"privateFlake": reference(private)}, None),
                ({"publicRevision": "fixture-revision-dirty"}, None),
                ({"user": "another-fixture"}, None),
                ({"localFile": "dotfiles.local.toml"}, 'value = "${local}"'),
                ({"localFile": "dotfiles.local.toml"}, 'value = "changed"'),
            ]:
                with self.subTest(changes=changes):
                    source, paths, evaluated = evaluate(changes, local)
                    self.assertNotEqual(source, original_source)
                    self.assertNotIn(tuple(paths), seen_paths)
                    seen_paths.add(tuple(paths))
                    self.assertTrue(evaluated)
                    self.assertEqual(evaluate(changes, local), (source, paths, False))
            (public / "dotfiles.toml").write_text('value = "changed"')
            source, paths, evaluated = evaluate({"publicFlake": reference(public)})
            changed_inventory = nix("build", "--dry-run", "--json", "--no-write-lock-file", "--no-update-lock-file", source + "#inventory")
            self.assertNotEqual(json.loads(inventory.stdout), json.loads(changed_inventory.stdout))
            self.assertIn("inventory-fixture-evaluated", changed_inventory.stderr)
            self.assertNotEqual(paths, original_paths)
            self.assertTrue(evaluated)
            self.assertEqual(evaluate(), (original_source, original_paths, False))
            derivation = json.loads(nix("derivation", "show", original_paths[0]).stdout)
            payload = json.loads(next(iter(derivation.values()))["env"]["payload"])
            self.assertEqual(payload["dotfilesDirectory"], inputs["directory"])
            self.assertIsNone(payload["configuration"]["local"])
            self.assertIsNone(payload["private"])


if __name__ == "__main__":
    unittest.main()
