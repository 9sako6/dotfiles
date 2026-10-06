{ pkgs, toolset }:
pkgs.linkFarm "dotfiles-system-resources" [
  { name = "bin/nightlight"; path = "${pkgs.lib.getBin toolset.nightlight}/bin/nightlight"; }
  { name = "share/anki-connect"; path = "${toolset.ankiConnect}/share/anki/addons/anki-connect"; }
]
