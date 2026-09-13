{ self, lib, pkgs }:
let
  make = module: self.lib.mkHost {
    dotfilesDirectory = "/fixture";
    primaryUser = "fixture";
    privateFlake.darwinModules.default = module;
  };
  composed = make {
    launchd.user.agents.fixture.serviceConfig = {
      ProgramArguments = [ "/usr/bin/true" ];
      RunAtLoad = true;
    };
    homebrew.taps = [ "fixture/tap" ];
  };
  rejects = module: !(builtins.tryEval (builtins.deepSeq (make module).system.drvPath true)).success;
  results = {
    buildable = (builtins.tryEval composed.system.drvPath).success;
    service = composed.config.launchd.user.agents.fixture.serviceConfig.RunAtLoad;
    tap = builtins.any (tap: tap.name == "fixture/tap") composed.config.homebrew.taps;
    configurationConflict = rejects { dotfiles.configuration = lib.mkForce { }; };
    copyConflict = rejects { home-manager.users.fixture.home.file.".claude/skills/foreign".text = "fixture"; };
    forcedCopyConflict = rejects { home-manager.users.fixture.home.file = lib.mkForce { ".gitconfig".text = "fixture"; }; };
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "private composition behavior test failed";
pkgs.writeText "composition-tests-passed" (builtins.toJSON results)
