{ pkgs }:
let
  addon = pkgs.writeTextDir "share/anki/addons/anki-connect/__init__.py" "fixture";
  fixtureToolset = {
    ankiConnect = addon;
  };
  fixture = import ../artifacts.nix { inherit pkgs; toolset = fixtureToolset; };
in
# Only tiny fixture packages are built.
pkgs.runCommand "artifact-constructor-check" {
  passthru.fixtureRoot = fixture;
} ''
  test -f ${fixture}/share/anki-connect/__init__.py
  test ! -e ${fixture}/bin/nightlight
  touch "$out"
''
