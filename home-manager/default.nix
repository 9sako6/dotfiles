{ config, dotfilesDirectory, dotfilesSourceHome, ... }:

let
  homeRoot = "${dotfilesDirectory}/home";
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
in
{
  imports = [ ./packages.nix ];

  home.stateVersion = "26.05";

  home.file = {
    ".gitconfig" = liveLink ".gitconfig";
    ".gitignore_global" = liveLink ".gitignore_global";
    ".zshenv" = liveLink ".zshenv";
    ".zshrc" = liveLink ".zshrc";
    "apm.lock.yaml" = liveLink "apm.lock.yaml";
    "apm.yml" = liveLink "apm.yml";
  } // liveFiles;
}
