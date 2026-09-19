import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]


class InventoryTests(unittest.TestCase):
    def test_inventory_uses_effective_declarations_without_building_or_exporting_job_secrets(self):
        expression = '''
          let
            public = builtins.getFlake ("git+file://" + builtins.getEnv "INVENTORY_REPOSITORY");
            configuration = (import (public.outPath + "/nix/configuration.nix") {
              inherit (public.inputs.nixpkgs) lib;
              publicFile = public.outPath + "/dotfiles.toml";
              localFile = null;
            }).config;
            host = public.lib.mkHost {
              inherit configuration;
              configurationRevision = "fixture";
              dotfilesDirectory = "/fixture";
              primaryUser = "fixture";
              privateFlake = {
                inputs.nixpkgs = public.inputs.nixpkgs;
                darwinModules.default = { lib, pkgs, ... }: {
                  _file = public.outPath + "/fixture-private.nix";
                  system.defaults.finder.AppleShowAllFiles = lib.mkForce false;
                  home-manager.users.fixture.home.packages = lib.mkAfter [ pkgs.hello ];
                  launchd.daemons.inventory-example.serviceConfig = {
                    EnvironmentVariables.SENSITIVE = "must-not-export";
                    ProgramArguments = [ "/fixture/program" "must-not-export" ];
                    RunAtLoad = false;
                    StartInterval = 42;
                  };
                  launchd.agents.inventory-shared.serviceConfig.RunAtLoad = true;
                };
              };
            };
          in import (public.outPath + "/nix/inventory.nix") {
            inherit configuration host;
            inputs = public.inputs;
            publicSource = public.outPath;
          }
        '''
        with tempfile.TemporaryDirectory() as temporary:
            environment = {**os.environ, "INVENTORY_REPOSITORY": str(REPOSITORY)}
            result = subprocess.run(
                ["nix", "eval", "--json", "--impure", "--no-write-lock-file",
                 "--no-update-lock-file", "--expr", expression],
                cwd=temporary, env=environment, capture_output=True, text=True, timeout=60,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("must-not-export", result.stdout)
        inventory = json.loads(result.stdout)
        settings = {setting["key"]: setting["value"] for setting in inventory["system"]}
        self.assertFalse(settings["system.defaults.finder.AppleShowAllFiles"])
        self.assertEqual(settings["system.defaults.finder.NewWindowTarget"], "Home")
        self.assertNotIn("system.keyboard.userKeyMapping", settings)
        self.assertTrue(any(p["name"] == "hello" and p["manager"] == "Nix" for p in inventory["packages"]))
        self.assertTrue(any(p["name"] == "node" and p["manager"] == "mise" for p in inventory["packages"]))
        job = next(job for job in inventory["services"] if job["name"] == "inventory-example")
        self.assertEqual(job["config"], {"RunAtLoad": False, "StartInterval": 42})
        shared = next(job for job in inventory["services"] if job["name"] == "inventory-shared")
        self.assertEqual(shared["scope"], "all users")
        self.assertFalse(inventory["localllm"]["enabled"])


if __name__ == "__main__":
    unittest.main()
