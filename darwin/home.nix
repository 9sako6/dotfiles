{ config, dotfilesDirectory, ... }:

let
  homeRoot = "${dotfilesDirectory}/home";
  outOfStore = relativePath:
    config.lib.file.mkOutOfStoreSymlink "${homeRoot}/${relativePath}";
  liveLink = relativePath: {
    source = outOfStore relativePath;
  };
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

    ".agents" = {
      source = outOfStore ".agents";
      recursive = true;
    };
    ".claude" = {
      source = outOfStore ".claude";
      recursive = true;
    };
    ".codex" = {
      source = outOfStore ".codex";
      recursive = true;
    };
  };
}
