{ self, lib, pkgs }:
let
  make = module: self.lib.mkHost {
    configurationRevision = "0123456789abcdef0123456789abcdef01234567-dirty";
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
  copyConfigured = self.lib.mkHost {
    configuration = composed.config.dotfiles.configuration // {
      copy = lib.sort builtins.lessThan (composed.config.dotfiles.configuration.copy ++ [ ".zshenv" ]);
    };
    dotfilesDirectory = "/fixture";
    primaryUser = "fixture";
  };
  cacheHost = self.lib.mkHost {
    configurationRevision = self.rev or self.dirtyRev or "unknown";
    dotfilesDirectory = "/cache-fixture";
    primaryUser = "cache-fixture";
    privateFlake.darwinModules.default.homebrew.casks = [ "fixture-cask" ];
  };
  codexGoHost = self.lib.mkHost {
    configuration = composed.config.dotfiles.configuration // { codex_go.enabled = true; };
    dotfilesDirectory = "/fixture";
    primaryUser = "fixture";
  };
  codexGoInventory = import ../inventory.nix {
    inherit (self) inputs;
    configuration = codexGoHost.config.dotfiles.configuration;
    host = codexGoHost;
    publicSource = self.outPath;
  };
  rejects = module: !(builtins.tryEval (builtins.deepSeq (make module).system.drvPath true)).success;
  results = {
    codexGoDisabled = !(composed.config.home-manager.users.fixture.launchd.agents ? codex-go);
    codexGoBuildable = (builtins.tryEval codexGoHost.system.drvPath).success;
    codexGoService = builtins.any (service: service.name == "codex-go" && service.config.RunAtLoad)
      codexGoInventory.services;
    cliRevision = (lib.findFirst (package: (package.pname or "") == "dotfiles") null
      composed.config.environment.systemPackages).DOTFILES_BUILD_REVISION
      == self.packages.${pkgs.stdenv.hostPlatform.system}.dotfiles.version;
    cliCacheReuse = (lib.findFirst (package: (package.pname or "") == "dotfiles") null
      cacheHost.config.environment.systemPackages).outPath == self.packages.${pkgs.stdenv.hostPlatform.system}.dotfiles.outPath;
    buildable = (builtins.tryEval composed.system.drvPath).success;
    service = composed.config.launchd.user.agents.fixture.serviceConfig.RunAtLoad;
    tap = builtins.any (tap: tap.name == "fixture/tap") composed.config.homebrew.taps;
    configurationConflict = rejects { dotfiles.configuration = lib.mkForce { }; };
    copiedFilesExcluded = !(composed.config.home-manager.users.fixture.home.file ? ".gitconfig");
    additionalCopiedFileExcluded = !(copyConfigured.config.home-manager.users.fixture.home.file ? ".zshenv");
    uncopiedFileLinked = composed.config.home-manager.users.fixture.home.file.".zshenv".enable;
    copyConflict = rejects { home-manager.users.fixture.home.file.".claude/skills/foreign".text = "fixture"; };
    copyTargetConflict = rejects { home-manager.users.fixture.home.file.alias = { target = ".gitconfig"; text = "fixture"; }; };
    copyParentConflict = rejects { home-manager.users.fixture.home.file.alias = { target = ".claude"; text = "fixture"; }; };
    forcedCopyConflict = rejects { home-manager.users.fixture.home.file = lib.mkForce { ".gitconfig".text = "fixture"; }; };
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "private composition behavior test failed";
pkgs.writeText "composition-tests-passed" (builtins.toJSON results)
