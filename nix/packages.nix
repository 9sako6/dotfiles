{ pkgs }:

let
  ankiConnectPackage = pkgs.ankiAddons.anki-connect;
  expectedAnkiConnectVersion = "25.11.9.0";

  nightlightPackage = pkgs.nightlight;
  expectedNightlightVersion = "1.0.0";


in
assert pkgs.lib.assertMsg (ankiConnectPackage.version == expectedAnkiConnectVersion)
  "AnkiConnect version drifted: expected ${expectedAnkiConnectVersion}, got ${ankiConnectPackage.version}";
assert pkgs.lib.assertMsg (nightlightPackage.version == expectedNightlightVersion)
  "Nightlight version drifted: expected ${expectedNightlightVersion}, got ${nightlightPackage.version}";

{
  ankiConnect = ankiConnectPackage;
  nightlight = nightlightPackage;
}
