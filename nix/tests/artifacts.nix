{ pkgs }:
let
  addon = pkgs.writeTextDir "share/anki/addons/anki-connect/__init__.py" "fixture";
  nightlight = pkgs.writeShellScriptBin "nightlight" "echo nightlight";
  fixtureToolset = {
    ankiConnect = addon;
    inherit nightlight;
  };
  fixture = import ../artifacts.nix { inherit pkgs; toolset = fixtureToolset; };
in
# Only tiny fixture packages are built.
pkgs.runCommand "artifact-constructor-check" {
  passthru.fixtureRoot = fixture;
} ''
  test -f ${fixture}/share/anki-connect/__init__.py
  test "$(${fixture}/bin/nightlight)" = nightlight
  touch "$out"
''
