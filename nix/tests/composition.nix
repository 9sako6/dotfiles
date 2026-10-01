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
    home-manager.users.fixture.home.file."fixture-owned" = {
      target = "/Users/fixture/fixture-owned";
      text = "fixture";
    };
  };
  enabledConfiguration = composed.config.dotfiles.configuration // {
    localllm = {
      enabled = true;
      models = [ "qwen3.8-9b-distill-4bit" ];
      default_model = "qwen3.8-9b-distill-4bit";
    };
  };
  artifactHost = configuration: module: self.lib.mkHost {
    inherit configuration;
    dotfilesDirectory = "/fixture";
    primaryUser = "fixture";
    privateFlake.darwinModules.default = module;
  };
  enabledHost = artifactHost enabledConfiguration { };
  rejectsArtifactFile = target: !(builtins.tryEval (builtins.deepSeq
    (artifactHost enabledConfiguration {
      home-manager.users.fixture.home.file.fixture = { inherit target; text = "fixture"; };
    }).system.drvPath true)).success;
  privatePackage = pkgs.writeShellScriptBin "private-fixture" "exit 0";
  privatePackageHost = artifactHost enabledConfiguration {
    home-manager.users.fixture.home.packages = [ privatePackage ];
  };
  copiedArtifactHost = artifactHost (enabledConfiguration // {
    copy = [ ".local/bin/localllm" "Library/Application Support/Anki2/addons21/anki-connect" ];
  }) { };
  inventory = builtins.fromJSON (builtins.unsafeDiscardStringContext composed.inventory.text);
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
  rejects = module: !(builtins.tryEval (builtins.deepSeq (make module).system.drvPath true)).success;
  results = {
    cliRevision = (lib.findFirst (package: (package.pname or "") == "dotfiles") null
      composed.config.environment.systemPackages).DOTFILES_BUILD_REVISION
      == self.packages.${pkgs.stdenv.hostPlatform.system}.dotfiles.version;
    cliCacheReuse = (lib.findFirst (package: (package.pname or "") == "dotfiles") null
      cacheHost.config.environment.systemPackages).outPath == self.packages.${pkgs.stdenv.hostPlatform.system}.dotfiles.outPath;
    buildable = (builtins.tryEval composed.system.drvPath).success;
    service = composed.config.launchd.user.agents.fixture.serviceConfig.RunAtLoad;
    tap = builtins.any (tap: tap.name == "fixture/tap") composed.config.homebrew.taps;
    artifactsAbsentFromPublicHome =
      !(enabledHost.config.home-manager.users.fixture.home.file ? "Library/Application Support/Anki2/addons21/anki-connect")
      && !(builtins.elem (self.lib.mkArtifacts { configuration = enabledConfiguration; }).selected.localllm.package
        enabledHost.config.home-manager.users.fixture.home.packages);
    remainingPublicPackagesPreserved = builtins.all (package:
      builtins.elem package enabledHost.config.home-manager.users.fixture.home.packages
    ) [ pkgs.anki-bin pkgs.ffmpeg pkgs.nightlight ];
    nightShiftPreserved = enabledHost.config.home-manager.users.fixture.home.activation ? configureNightShift;
    privatePackagePreserved = builtins.elem privatePackage privatePackageHost.config.home-manager.users.fixture.home.packages;
    artifactFileConflict = rejectsArtifactFile "Library/Application Support/Anki2/addons21/anki-connect";
    artifactFileParentConflict = rejectsArtifactFile "Library/Application Support/Anki2/addons21";
    artifactFileChildConflict = rejectsArtifactFile "Library/Application Support/Anki2/addons21/anki-connect/config.json";
    artifactExecutableConflict = rejectsArtifactFile ".local/bin/localllm";
    artifactAbsoluteTargetConflict = rejectsArtifactFile "/Users/fixture/.local/bin/localllm";
    disabledPrivateFilePreserved = (builtins.tryEval (artifactHost enabledConfiguration {
      home-manager.users.fixture.home.file.fixture = {
        target = ".local/bin/localllm";
        enable = false;
        text = "fixture";
      };
    }).system.drvPath).success;
    copiedArtifactBuildable = (builtins.tryEval copiedArtifactHost.system.drvPath).success;
    exactHomeManagerPackageProfile = inventory.homeManagerPackageProfile
      == toString composed.config.home-manager.users.fixture.home.path;
    exactHomeManagerPackageProfileDrv = inventory.homeManagerPackageProfileDrv
      == composed.config.home-manager.users.fixture.home.path.drvPath;
    configurationConflict = rejects { dotfiles.configuration = lib.mkForce { }; };
    copiedFilesExcluded = !(composed.config.home-manager.users.fixture.home.file ? ".gitconfig");
    additionalCopiedFileExcluded = !(copyConfigured.config.home-manager.users.fixture.home.file ? ".zshenv");
    publicLiveFileExcluded = !(composed.config.home-manager.users.fixture.home.file ? ".zshenv");
    privateHomeTargetListed = builtins.elem "fixture-owned" (builtins.fromJSON (builtins.unsafeDiscardStringContext composed.inventory.text)).homeManagerTargets;
    privateHomeFilePreserved = composed.config.home-manager.users.fixture.home.file."fixture-owned".enable;
    copyConflict = rejects { home-manager.users.fixture.home.file.".claude/skills/foreign".text = "fixture"; };
    copyTargetConflict = rejects { home-manager.users.fixture.home.file.alias = { target = ".gitconfig"; text = "fixture"; }; };
    copyParentConflict = rejects { home-manager.users.fixture.home.file.alias = { target = ".claude"; text = "fixture"; }; };
    forcedCopyConflict = rejects { home-manager.users.fixture.home.file = lib.mkForce { ".gitconfig".text = "fixture"; }; };
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results)) "private composition behavior test failed";
pkgs.writeText "composition-tests-passed" (builtins.toJSON results)
