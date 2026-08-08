{ config, dotfilesDirectory, ... }:

let
  sourceHome = ../home;
  homeRoot = "${dotfilesDirectory}/home";
  outOfStore = relativePath:
    config.lib.file.mkOutOfStoreSymlink "${homeRoot}/${relativePath}";
  liveLink = relativePath: {
    source = outOfStore relativePath;
  };
  collectLiveFiles = relativeRoot: sourceRoot:
    builtins.foldl'
      (files: name:
        let
          entryType = (builtins.readDir sourceRoot).${name};
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
      (builtins.attrNames (builtins.readDir sourceRoot));
  mutableTrees = builtins.foldl'
    (files: relativeRoot:
      files // collectLiveFiles relativeRoot (sourceHome + "/${relativeRoot}"))
    { }
    [
      ".agents"
      ".claude"
      ".codex"
    ];
in
{
  home.stateVersion = "26.05";

  home.file = {
    ".config" = liveLink ".config";
    ".gitconfig" = liveLink ".gitconfig";
    ".gitignore_global" = liveLink ".gitignore_global";
    ".zsh.d" = liveLink ".zsh.d";
    ".zshenv" = liveLink ".zshenv";
    ".zshrc" = liveLink ".zshrc";
    "apm.lock.yaml" = liveLink "apm.lock.yaml";
    "apm.yml" = liveLink "apm.yml";
    "mybin" = liveLink "mybin";
  } // mutableTrees;
}
