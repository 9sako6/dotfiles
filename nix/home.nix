{ config, lib, pkgs, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  ankiConnectAddon = "${toolset.ankiConnect}/share/anki/addons/anki-connect";
  homeRoot = "${dotfilesDirectory}/home";
  toolset = import ./packages.nix { inherit pkgs; };
  outOfStore = relativePath:
    config.lib.file.mkOutOfStoreSymlink "${homeRoot}/${relativePath}";
  liveLink = relativePath: {
    source = outOfStore relativePath;
  };
  collectLiveFiles = relativeRoot: sourceRoot:
    let
      entries = builtins.readDir sourceRoot;
    in
    builtins.foldl'
      (files: name:
        let
          entryType = entries.${name};
          relativePath = "${relativeRoot}/${name}";
          sourcePath = sourceRoot + "/${name}";
        in
        files // (
          if entryType == "directory" then
            collectLiveFiles relativePath sourcePath
          else
            { ${relativePath} = liveLink relativePath; }
        ))
      { }
      (builtins.attrNames entries);
  liveFiles = builtins.foldl'
    (files: relativeRoot:
      files // collectLiveFiles relativeRoot (dotfilesSourceHome + "/${relativeRoot}"))
    { }
    [
      ".config"
      ".zsh.d"
      "mybin"
    ];
  nightShift = {
    schedule = {
      start = "22:00";
      end = "07:00";
    };
    temperature = 80;
  };
in
{
  home.activation.configureNightShift = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    ${pkgs.nightlight}/bin/nightlight schedule ${nightShift.schedule.start} ${nightShift.schedule.end}
    ${pkgs.nightlight}/bin/nightlight temp ${toString nightShift.temperature}
  '';

  home.stateVersion = "26.05";
  home.packages = toolset.packages;

  home.file = {
    ".gitconfig" = liveLink ".gitconfig";
    ".gitignore_global" = liveLink ".gitignore_global";
    ".zshenv" = liveLink ".zshenv";
    ".zshrc" = liveLink ".zshrc";
    "Library/Application Support/Anki2/addons21/anki-connect".source = ankiConnectAddon;
    "apm.lock.yaml" = liveLink "apm.lock.yaml";
    "apm.yml" = liveLink "apm.yml";
  } // liveFiles;
}
