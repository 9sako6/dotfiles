import json
import os
from pathlib import Path
import plistlib
import shlex
import shutil
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]


class InventoryTests(unittest.TestCase):
    def test_generation_keeps_its_inventory_and_frozen_skill_source_reachable(self):
        expression = '''
          let
            public = builtins.getFlake ("git+file://" + builtins.getEnv "INVENTORY_REPOSITORY");
            host = public.lib.mkHost {
              configurationRevision = "fixture";
              dotfilesDirectory = "/fixture";
              primaryUser = "fixture";
              privateFlake.darwinModules.default = { lib, ... }: {
                _file = public.outPath + "/fixture-private.nix";
                system.defaults.dock.show-recents = lib.mkForce true;
              };
            };
          in host.pkgs.runCommand "dotfiles-generation-fixture" {} ''
            mkdir -p "$out"
            ${host.config.system.systemBuilderCommands}
          ''
        '''
        result = subprocess.run(
            ["nix", "build", "--no-link", "--json", "--impure", "--no-write-lock-file",
             "--no-update-lock-file", "--expr", expression],
            env={**os.environ, "INVENTORY_REPOSITORY": str(REPOSITORY)},
            capture_output=True, text=True, timeout=120,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        generation = Path(json.loads(result.stdout)[0]["outputs"]["out"])
        snapshot = generation / "dotfiles-inventory.json"
        self.assertTrue(snapshot.is_symlink())
        inventory = json.loads(snapshot.read_text())
        values = {setting["key"]: setting["value"] for setting in inventory["system"]}
        self.assertTrue(values["system.defaults.dock.show-recents"])
        self.assertEqual(inventory["schemaVersion"], 3)
        self.assertEqual(values["nix.gc.options"], "--delete-older-than 2d")
        self.assertTrue(values["nix.gc.automatic"])
        self.assertFalse(values["homebrew.global.autoUpdate"])
        self.assertFalse(values["homebrew.onActivation.autoUpdate"])
        self.assertEqual(values["homebrew.onActivation.cleanup"], "uninstall")
        self.assertFalse(values["homebrew.onActivation.upgrade"])
        self.assertFalse(values["nix-homebrew.mutableTaps"])
        packages = {package["name"]: package["declared"] for package in inventory["packages"]}
        self.assertEqual(packages["dotfiles"], "fixture")
        self.assertTrue(packages["lix"])
        self.assertTrue(packages["zundamonotify"])
        self.assertEqual(values["nightShift.schedule.start"], "22:00")
        self.assertEqual(values["nightShift.schedule.end"], "07:00")
        self.assertEqual(values["nightShift.temperature"], 80)
        self.assertTrue(values["dictationShortcut.enabled"])
        self.assertEqual(values["dictationShortcut.parameters"], ["1048576", "18446744073708503039"])
        self.assertEqual(values["dictationShortcut.type"], "modifier")
        source = Path(inventory["source"])
        self.assertTrue((source / "home/apm.yml").is_file())
        self.assertTrue((source / "home/.agents/skills/jp/SKILL.md").is_file())
        references = subprocess.check_output(
            ["nix-store", "--query", "--requisites", str(generation)], text=True,
        ).splitlines()
        self.assertIn(str(snapshot.resolve()), references)
        self.assertIn(str(source), references)

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
                  environment.systemPackages = lib.mkAfter [ pkgs.hello ];
                  homebrew.global.autoUpdate = lib.mkForce true;
                  homebrew.onActivation = {
                    autoUpdate = lib.mkForce true;
                    cleanup = lib.mkForce "zap";
                    upgrade = lib.mkForce true;
                  };
                  home-manager.users.fixture.home.packages = lib.mkAfter [ pkgs.hello ];
                  launchd.daemons.inventory-example.serviceConfig = {
                    EnvironmentVariables.SENSITIVE = "must-not-export";
                    ProgramArguments = [ "/fixture/program" "must-not-export" ];
                    RunAtLoad = false;
                    StartInterval = 42;
                  };
                  launchd.agents.inventory-shared.serviceConfig.RunAtLoad = true;
                  nix.gc = {
                    automatic = lib.mkForce false;
                    options = lib.mkForce "--delete-older-than 7d";
                  };
                  nix.package = lib.mkForce (pkgs.lix.overrideAttrs { version = "9.8.7"; });
                  nix-homebrew.mutableTaps = lib.mkForce true;
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
        for package in inventory["packages"]:
            self.assertEqual(set(package), {"name", "manager", "declared"})
        settings = {setting["key"]: setting["value"] for setting in inventory["system"]}
        self.assertFalse(settings["system.defaults.finder.AppleShowAllFiles"])
        self.assertEqual(settings["system.defaults.finder.NewWindowTarget"], "Home")
        self.assertNotIn("system.keyboard.userKeyMapping", settings)
        self.assertEqual(settings["nix.gc.options"], "--delete-older-than 7d")
        self.assertFalse(settings["nix.gc.automatic"])
        self.assertTrue(settings["homebrew.global.autoUpdate"])
        self.assertTrue(settings["homebrew.onActivation.autoUpdate"])
        self.assertEqual(settings["homebrew.onActivation.cleanup"], "zap")
        self.assertTrue(settings["homebrew.onActivation.upgrade"])
        self.assertTrue(settings["nix-homebrew.mutableTaps"])
        self.assertIn({"name": "lix", "manager": "Nix", "declared": "9.8.7"}, inventory["packages"])
        self.assertTrue(any(p["name"] == "hello" and p["manager"] == "Nix" for p in inventory["packages"]))
        self.assertTrue(any(p["name"] == "node" and p["manager"] == "mise" for p in inventory["packages"]))
        job = next(job for job in inventory["services"] if job["name"] == "inventory-example")
        self.assertEqual(job["config"], {"RunAtLoad": False, "StartInterval": 42})
        shared = next(job for job in inventory["services"] if job["name"] == "inventory-shared")
        self.assertEqual(shared["scope"], "all users")
        self.assertFalse(inventory["localllm"]["enabled"])

    def test_activation_commands_and_inventory_follow_the_same_setting_changes(self):
        expression = '''
          let
            public = builtins.getFlake (builtins.getEnv "INVENTORY_FLAKE");
            host = public.lib.mkHost { dotfilesDirectory = "/fixture"; primaryUser = "fixture"; };
          in {
            source = public.outPath;
            inventory = host.inventory.text;
            nightShift = host.config.home-manager.users.fixture.home.activation.configureNightShift.data;
            dictation = host.config.system.activationScripts.postActivation.text;
          }
        '''

        def evaluate(reference):
            result = subprocess.run(
                ["nix", "eval", "--json", "--impure", "--no-write-lock-file",
                 "--no-update-lock-file", "--expr", expression],
                env={**os.environ, "INVENTORY_FLAKE": reference},
                capture_output=True, text=True, timeout=60,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            snapshot = json.loads(result.stdout)
            snapshot["inventory"] = json.loads(snapshot["inventory"])
            return snapshot

        original = evaluate("git+file://" + str(REPOSITORY))
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "source"
            shutil.copytree(original["source"], source)
            settings_file = source / "nix/macos-settings.nix"
            settings_file.chmod(0o644)
            settings_file.write_text('''{
              dictationShortcut = {
                enabled = false;
                parameters = [ "131072" "42" ];
                type = "standard";
              };
              nightShift = {
                schedule = { start = "21:15"; end = "08:30"; };
                temperature = 65;
              };
            }''')
            changed = evaluate("path:" + str(source))

        for snapshot, start, end, temperature, enabled, parameters, kind in [
            (original, "22:00", "07:00", 80, True, [1048576, 18446744073708503039], "modifier"),
            (changed, "21:15", "08:30", 65, False, [131072, 42], "standard"),
        ]:
            with self.subTest(start=start):
                values = {setting["key"]: setting["value"] for setting in snapshot["inventory"]["system"]}
                commands = [shlex.split(line) for line in snapshot["nightShift"].splitlines() if line.strip()]
                self.assertEqual(commands[0][1:], ["schedule", start, end])
                self.assertEqual(commands[1][1:], ["temp", str(temperature)])
                self.assertEqual(values["nightShift.schedule.start"], start)
                self.assertEqual(values["nightShift.schedule.end"], end)
                self.assertEqual(values["nightShift.temperature"], temperature)
                command = shlex.split(snapshot["dictation"].replace("\\\n", ""))
                self.assertEqual(command[-3:-1], ["-dict-add", "164"])
                shortcut = plistlib.loads(("<plist>" + command[-1] + "</plist>").encode())
                self.assertEqual(shortcut, {"enabled": enabled, "value": {"parameters": parameters, "type": kind}})
                self.assertEqual(values["dictationShortcut.enabled"], enabled)
                self.assertEqual(values["dictationShortcut.parameters"], [str(value) for value in parameters])
                self.assertEqual(values["dictationShortcut.type"], kind)


if __name__ == "__main__":
    unittest.main()
