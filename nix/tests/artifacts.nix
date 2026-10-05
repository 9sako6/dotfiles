{ pkgs }:
let
  lib = pkgs.lib;
  addon = pkgs.writeTextDir "share/anki/addons/anki-connect/__init__.py" "fixture";
  nightlight = pkgs.writeShellScriptBin "nightlight" "echo nightlight";
  fixtureToolset = {
    ankiConnect = addon;
    inherit nightlight;
  };
  fixture = import ../artifacts.nix { inherit pkgs; toolset = fixtureToolset; };
  results = {
    fixtureSelectsResources = builtins.attrNames fixture.selected == [ "anki-connect" "nightlight" ];
  };
in
assert lib.assertMsg (builtins.all (value: value) (builtins.attrValues results))
  "artifact constructor contract test failed";
# Only tiny fixture packages are built.
pkgs.runCommand "artifact-constructor-check" {
  passthru.fixtureRoot = fixture.root;
} ''
  test -f ${fixture.root}/share/anki-connect/__init__.py
  test "$(${fixture.root}/bin/nightlight)" = nightlight
  printf '%s\n' ${lib.escapeShellArg (builtins.toJSON results)} > "$out"
''
