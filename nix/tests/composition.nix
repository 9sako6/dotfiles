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
      home.file."related-private" = { target = ".config/git/private.conf"; text = "fixture"; };
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
  publicHost = self.lib.mkHost { dotfilesDirectory = "/fixture"; primaryUser = "fixture"; };
  otherMachine = self.lib.mkHost {
    dotfilesDirectory = "/another-fixture";
    primaryUser = "another-fixture";
    privateFlake.darwinModules.default.home-manager.users.another-fixture.home = {
      file."machine-owned".text = "another machine";
      stateVersion = "24.05";
    };
  };
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
    forcedCopyConflict = rejects {
      home-manager.users.fixture.home.file = lib.mkForce { ".gitconfig".text = "fixture"; };
    };
    noPublicHomeManagerUser = publicHost.config.home-manager.users == { };
    noPublicUserPackagesInSystem = builtins.all (package:
      !(builtins.elem (lib.getName package) [ "dotfiles" "ffmpeg" "localllm" "nightlight" ])
    ) publicHost.config.environment.systemPackages;
    otherMachinePreserved = (builtins.tryEval otherMachine.system.drvPath).success
      && otherMachine.config.home-manager.users.another-fixture.home.stateVersion == "24.05"
      && otherMachine.config.home-manager.users.another-fixture.home.file."machine-owned".enable;
    privateHomeFilePreserved = composed.config.home-manager.users.fixture.home.file."fixture-owned".enable;
    privateSiblingPreserved = composed.config.home-manager.users.fixture.home.file."related-private".enable;
    privatePackagePreserved = builtins.elem privatePackage composed.config.home-manager.users.fixture.home.packages;
    privateUserAgentPreserved = composed.config.home-manager.users.fixture.launchd.agents.private-fixture.config.RunAtLoad;
    publicLinkConflict = rejects { home-manager.users.fixture.home.file.alias = { target = ".config/mise"; text = "fixture"; }; };
    publicResourceConflict = rejects { home-manager.users.fixture.home.file.alias = { target = ".local/bin/nightlight"; text = "fixture"; }; };
    publicActivationAbsent = !(composed.config.home-manager.users.fixture.home.activation ? configureNightShift);
    publicHomeFilesAbsent = builtins.all (path: !(builtins.hasAttr path composed.config.home-manager.users.fixture.home.file))
      [ ".gitconfig" ".zshenv" "Library/Application Support/Anki2/addons21/anki-connect" ];
    service = composed.config.launchd.user.agents.fixture.serviceConfig.RunAtLoad;
    tap = builtins.any (tap: tap.name == "fixture/tap") composed.config.homebrew.taps;
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "private composition behavior test failed";
pkgs.writeText "composition-tests-passed" (builtins.toJSON results)
