{ pkgs, inputs }:

let
  ankiConnectPackage = pkgs.ankiAddons.anki-connect;
  expectedAnkiConnectVersion = "25.11.9.0";

  bunPackage = pkgs.bun;
  expectedBunVersion = "1.3.13";

  cachixPackage = pkgs.cachix;
  expectedCachixVersion = "1.11.1";

  gitPackage = pkgs.git;
  expectedGitVersion = "2.55.0";

  nightlightPackage = pkgs.nightlight;
  expectedNightlightVersion = "1.0.0";


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

{
  ankiConnect = ankiConnectPackage;
  cachix = cachixPackage;
  ciPackages = [
    # Bun 1.3.13
    bunPackage

    # Git 2.55.0
    gitPackage

  ];
  localllm = configuration: import ./localllm/package.nix { inherit configuration inputs pkgs; };
  localllmClient = import ./localllm/client.nix { inherit pkgs; };
  localllmGoalPlugin = import ./localllm/goal-plugin.nix { inherit pkgs; };
  localllmRuntime = (import ./localllm/runtime.nix { inherit inputs pkgs; }).environment;
  nightlight = nightlightPackage;
}
