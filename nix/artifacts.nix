{ pkgs, toolset }:
pkgs.linkFarm "dotfiles-system-resources" [
  { name = "share/anki-connect"; path = "${toolset.ankiConnect}/share/anki/addons/anki-connect"; }
]
