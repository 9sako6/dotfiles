{ self, pkgs }:
let
  lib = pkgs.lib;
  addon = pkgs.writeTextDir "share/anki/addons/anki-connect/__init__.py" "fixture";
  nightlight = pkgs.writeShellScriptBin "nightlight" "echo nightlight";
  fixtureToolset = {
    ankiConnect = addon;
    inherit nightlight;
  };
  fixture = import ../artifacts.nix { inherit pkgs; toolset = fixtureToolset; };
  actual = self.lib.mkArtifacts { };
  results = {
    fixtureSelectsResources = builtins.attrNames fixture.selected == [ "anki-connect" "nightlight" ];
    nightlightKeepsFixedPackage = actual.selected.nightlight.drvPath
      == (lib.getBin pkgs.nightlight).drvPath;
    publicOutputMatchesConstructor = self.packages.${pkgs.stdenv.hostPlatform.system}.artifacts.drvPath
      == actual.root.drvPath;
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results))
  "artifact constructor contract test failed";
# Only tiny fixture packages are built. The real packages above are evaluated,
# never interpolated into build inputs, so no full artifact build is requested.
pkgs.runCommand "artifact-constructor-check" {
  passthru.fixtureRoot = fixture.root;
} ''
  test -f ${fixture.root}/share/anki-connect/__init__.py
  test "$(${fixture.root}/bin/nightlight)" = nightlight
  printf '%s\n' ${lib.escapeShellArg (builtins.toJSON results)} > "$out"
''
