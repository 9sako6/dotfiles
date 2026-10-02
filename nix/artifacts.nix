{ pkgs, toolset }:
let
  selected = {
    anki-connect = toolset.ankiConnect;
    nightlight = pkgs.lib.getBin toolset.nightlight;
  };
  root = pkgs.linkFarm "dotfiles-system-resources" (
    [ { name = "share/anki-connect"; path = "${selected.anki-connect}/share/anki/addons/anki-connect"; } ]
    ++ map (name: { name = "bin/${name}"; path = "${selected.${name}}/bin/${name}"; })
      (builtins.filter (name: name != "anki-connect") (builtins.attrNames selected))
  );
in
{ inherit root selected; }
