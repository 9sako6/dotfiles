{ pkgs }:

let
  ankiConnectPackage = pkgs.ankiAddons.anki-connect;
  expectedAnkiConnectVersion = "25.11.9.0";

in
assert pkgs.lib.assertMsg (ankiConnectPackage.version == expectedAnkiConnectVersion)
  "AnkiConnect version drifted: expected ${expectedAnkiConnectVersion}, got ${ankiConnectPackage.version}";

{
  ankiConnect = ankiConnectPackage;
}
