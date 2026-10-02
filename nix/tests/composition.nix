{ self, lib, pkgs }:
let
  defaultConfiguration = self.darwinConfigurations.current.config.dotfiles.configuration;
  make = module: self.lib.mkHost {
    configurationRevision = "0123456789abcdef0123456789abcdef01234567-dirty";
    dotfilesDirectory = "/fixture";
    primaryUser = "fixture";
    privateFlake.darwinModules.default = module;
  };
  privatePackage = pkgs.writeShellScriptBin "private-fixture" "exit 0";
  composed = make {
    home-manager.users.fixture = { configuration, dotfilesDirectory, dotfilesSourceHome, inputs, ... }: {
      assertions = [ {
        assertion = configuration == defaultConfiguration
          && dotfilesDirectory == "/fixture"
          && dotfilesSourceHome == self.outPath + "/home"
          && inputs.nixpkgs.outPath == self.inputs.nixpkgs.outPath;
        message = "private Home Manager special arguments changed";
      } ];
      home.file."fixture-owned" = {
        target = "/Users/fixture/fixture-owned";
        text = "fixture";
      };
      home.packages = [ privatePackage ];
      launchd.agents.private-fixture = {
        enable = true;
        config = {
          ProgramArguments = [ "${privatePackage}/bin/private-fixture" ];
          RunAtLoad = true;
        };
      };
    };
    homebrew.taps = [ "fixture/tap" ];
    launchd.user.agents.fixture.serviceConfig = {
      ProgramArguments = [ "/usr/bin/true" ];
      RunAtLoad = true;
    };
  };
  inventory = builtins.fromJSON (builtins.unsafeDiscardStringContext composed.inventory.text);
  publicHost = self.lib.mkHost { dotfilesDirectory = "/fixture"; primaryUser = "fixture"; };
  publicInventory = builtins.fromJSON (builtins.unsafeDiscardStringContext publicHost.inventory.text);
  otherMachine = self.lib.mkHost {
    dotfilesDirectory = "/another-fixture";
    primaryUser = "another-fixture";
    privateFlake.darwinModules.default.home-manager.users.another-fixture.home = {
      file."machine-owned".text = "another machine";
      stateVersion = "24.05";
    };
  };
  # Match the Rust system projection: deliberately omit all public user files,
  # CLI sources, artifact recipes, package recipes and model configuration.
  systemFiles = [
    "flake.lock"
    "flake.nix"
    "nix/default.nix"
    "nix/home.nix"
    "nix/homebrew-packages.nix"
    "nix/homebrew-shellenv.zsh"
    "nix/host-flake.nix"
    "nix/inventory.nix"
    "nix/macos-settings.nix"
    "nix/system.nix"
  ];
  systemSource = builtins.path {
    name = "dotfiles-system-fixture";
    path = self.outPath;
    filter = path: type:
      let relative = lib.removePrefix (toString self.outPath + "/") (toString path);
      in builtins.elem relative systemFiles
        || (type == "directory" && (toString path == toString self.outPath || relative == "nix"));
  };
  projectedOutputs = (import (systemSource + "/flake.nix")).outputs (self.inputs // {
    self = projectedOutputs // { outPath = systemSource; inputs = self.inputs; };
  });
  projectedHost = projectedOutputs.lib.mkHost {
    configuration = defaultConfiguration;
    dotfilesDirectory = "/fixture";
    primaryUser = "fixture";
  };
  projectedInventory = builtins.fromJSON (builtins.unsafeDiscardStringContext projectedHost.inventory.text);
  rejects = module: !(builtins.tryEval (builtins.deepSeq (make module).system.drvPath true)).success;
  results = {
    buildable = (builtins.tryEval composed.system.drvPath).success;
    configurationConflict = rejects { dotfiles.configuration = lib.mkForce { }; };
    copyAbsoluteTargetConflict = rejects {
      home-manager.users.fixture.home.file.alias = { target = "/Users/fixture/.gitconfig"; text = "fixture"; };
    };
    copyConflict = rejects { home-manager.users.fixture.home.file.".claude/skills/foreign".text = "fixture"; };
    copyParentConflict = rejects {
      home-manager.users.fixture.home.file.alias = { target = ".claude"; text = "fixture"; };
    };
    copyTargetConflict = rejects {
      home-manager.users.fixture.home.file.alias = { target = ".gitconfig"; text = "fixture"; };
    };
    exactHomeManagerPackageProfile = inventory.homeManagerPackageProfile
      == toString composed.config.home-manager.users.fixture.home.path;
    exactHomeManagerPackageProfileDrv = inventory.homeManagerPackageProfileDrv
      == composed.config.home-manager.users.fixture.home.path.drvPath;
    forcedCopyConflict = rejects {
      home-manager.users.fixture.home.file = lib.mkForce { ".gitconfig".text = "fixture"; };
    };
    noPublicHomeManagerUser = publicHost.config.home-manager.users == { };
    noPublicHomeProfile = publicInventory.homeManagerPackageProfile == null
      && publicInventory.homeManagerPackageProfileDrv == null
      && publicInventory.homeManagerTargets == [ ];
    noPublicUserPackagesInSystem = builtins.all (package:
      !(builtins.elem (lib.getName package) [ "dotfiles" "ffmpeg" "localllm" "nightlight" ])
    ) publicHost.config.environment.systemPackages;
    noRootlessInventory = publicInventory.tools == [ ]
      && builtins.all (package: package.manager != "mise") publicInventory.packages
      && builtins.all (setting: !(lib.hasPrefix "nightShift." setting.key)) publicInventory.system;
    otherMachinePreserved = (builtins.tryEval otherMachine.system.drvPath).success
      && otherMachine.config.home-manager.users.another-fixture.home.stateVersion == "24.05"
      && otherMachine.config.home-manager.users.another-fixture.home.file."machine-owned".enable;
    privateHomeFilePreserved = composed.config.home-manager.users.fixture.home.file."fixture-owned".enable;
    privateHomeTargetListed = builtins.elem "fixture-owned" inventory.homeManagerTargets;
    privatePackagePreserved = builtins.elem privatePackage composed.config.home-manager.users.fixture.home.packages;
    privateUserAgentPreserved = composed.config.home-manager.users.fixture.launchd.agents.private-fixture.config.RunAtLoad
      && builtins.any (service: service.name == "private-fixture" && service.scope == "user") inventory.services;
    projectedSourceBuildable = (builtins.tryEval projectedHost.system.drvPath).success;
    projectedSourceHasNoRootlessInputs = builtins.all (path: !(builtins.pathExists (systemSource + "/${path}")))
      [ ".mise.toml" "cli" "dotfiles.toml" "home" "nix/artifacts.nix" "nix/configuration.nix" "nix/localllm" "nix/packages.nix" ];
    projectedSourceInventory = projectedInventory.source == toString systemSource
      && projectedInventory.schemaVersion == 4;
    publicActivationAbsent = !(composed.config.home-manager.users.fixture.home.activation ? configureNightShift);
    publicHomeFilesAbsent = builtins.all (path: !(builtins.hasAttr path composed.config.home-manager.users.fixture.home.file))
      [ ".gitconfig" ".zshenv" "Library/Application Support/Anki2/addons21/anki-connect" ];
    service = composed.config.launchd.user.agents.fixture.serviceConfig.RunAtLoad;
    tap = builtins.any (tap: tap.name == "fixture/tap") composed.config.homebrew.taps;
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "private composition behavior test failed";
pkgs.writeText "composition-tests-passed" (builtins.toJSON results)
