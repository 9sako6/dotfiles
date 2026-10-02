{ pkgs, inputs }:

let
  ankiConnectPackage = pkgs.ankiAddons.anki-connect;
  expectedAnkiConnectVersion = "25.11.9.0";

  bunPackage = pkgs.bun;
  expectedBunVersion = "1.3.13";

  cachixPackage = pkgs.cachix;
  expectedCachixVersion = "1.11.1";

  dotfilesSource = builtins.path { path = ../cli; name = "dotfiles-cli-source"; };
  dotfilesRevision = "source-${builtins.substring 0 32 (builtins.unsafeDiscardStringContext (builtins.baseNameOf dotfilesSource))}";
  dotfilesPackage = pkgs.rustPlatform.buildRustPackage {
    pname = "dotfiles";
    version = dotfilesRevision;
    src = dotfilesSource;
    cargoLock.lockFile = ../cli/Cargo.lock;
    nativeCheckInputs = [ pkgs.git ];
    env.DOTFILES_BUILD_REVISION = dotfilesRevision;
  };

  gitPackage = pkgs.git;
  expectedGitVersion = "2.55.0";

  nightlightPackage = pkgs.nightlight;
  expectedNightlightVersion = "1.0.0";

  rustToolchain = pkgs.rustPackages_1_97;
  expectedRustVersion = "1.97.1";

in
assert pkgs.lib.assertMsg (ankiConnectPackage.version == expectedAnkiConnectVersion)
  "AnkiConnect version drifted: expected ${expectedAnkiConnectVersion}, got ${ankiConnectPackage.version}";
assert pkgs.lib.assertMsg (bunPackage.version == expectedBunVersion)
  "Bun version drifted: expected ${expectedBunVersion}, got ${bunPackage.version}";
assert pkgs.lib.assertMsg (cachixPackage.version == expectedCachixVersion)
  "Cachix version drifted: expected ${expectedCachixVersion}, got ${cachixPackage.version}";
assert pkgs.lib.assertMsg (gitPackage.version == expectedGitVersion)
  "Git version drifted: expected ${expectedGitVersion}, got ${gitPackage.version}";
assert pkgs.lib.assertMsg (nightlightPackage.version == expectedNightlightVersion)
  "Nightlight version drifted: expected ${expectedNightlightVersion}, got ${nightlightPackage.version}";
assert pkgs.lib.assertMsg (rustToolchain.rustc.version == expectedRustVersion)
  "Rust version drifted: expected ${expectedRustVersion}, got ${rustToolchain.rustc.version}";
{
  ankiConnect = ankiConnectPackage;
  cachix = cachixPackage;
  ciPackages = [
    # Bun 1.3.13
    bunPackage

    # Git 2.55.0
    gitPackage

    # Rust 1.97.1
    rustToolchain.rustc
    rustToolchain.cargo
    rustToolchain.rustfmt
    rustToolchain.clippy
  ];
  dotfiles = dotfilesPackage;
  localllm = configuration: import ./localllm/package.nix { inherit configuration inputs pkgs; };
  localllmClient = import ./localllm/client.nix { inherit pkgs; };
  localllmGoalPlugin = import ./localllm/goal-plugin.nix { inherit pkgs; };
  localllmRuntime = (import ./localllm/runtime.nix { inherit inputs pkgs; }).environment;
  nightlight = nightlightPackage;
}
